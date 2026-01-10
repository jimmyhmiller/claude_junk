//! Generational GC Runtime with LLVM Stackmap-based Root Scanning
//!
//! A generational collector with:
//! - Young generation: bump-pointer allocation with copying collection
//! - Old generation: mark-and-sweep with free list
//!
//! Uses the same LLVM stackmap infrastructure as gc_runtime.rs

use std::cell::RefCell;
use std::collections::HashMap;
use std::ptr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::ffi::c_void;

use crate::tagged_value::*;
use crate::stackmap::{StackMap, Location, LocationType};

use stack_walker::{StackWalker, UnsafeDirectReader, WalkConfig};

#[cfg(target_arch = "aarch64")]
use stack_walker::{Aarch64StackWalker, UnwindRegsAarch64};

#[cfg(target_arch = "x86_64")]
use stack_walker::{UnwindRegsX86_64, X86_64StackWalker};

// Static storage for stackmap and stack bounds
static STACK_LOW: AtomicUsize = AtomicUsize::new(0);
static STACK_HIGH: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    static STACKMAP: RefCell<Option<StackMap>> = const { RefCell::new(None) };
    static SAFEPOINT_MAP: RefCell<HashMap<u64, usize>> = RefCell::new(HashMap::new());
}

/// Get page size
fn get_page_size() -> usize {
    unsafe { libc::sysconf(libc::_SC_PAGESIZE) as usize }
}

/// Size configuration
fn young_gen_size() -> usize {
    std::env::var("GC_YOUNG_MB")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(16) * 1024 * 1024
}

fn old_gen_initial_pages() -> usize {
    std::env::var("GC_OLD_PAGES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(4096) // ~16MB at 4KB pages
}

const MAX_OLD_GEN_PAGES: usize = 1000000;

/// Find the function containing a given PC address.
/// Uses MMTk's approach: find the function with the highest start address <= pc.
/// This works because functions are laid out sequentially in memory.
fn function_for_pc(stackmap: &StackMap, pc: u64) -> Option<&crate::stackmap::StackMapFunction> {
    stackmap
        .functions
        .iter()
        .filter(|func| pc >= func.address)
        .max_by_key(|func| func.address)
}

/// Frame layout information
struct FrameLayout {
    stack_size: usize,
    fp_offset: usize,
    lr_offset: usize,
}

fn frame_layout_from_stack_size(stack_size: usize) -> Option<FrameLayout> {
    if stack_size < 16 || stack_size % 16 != 0 {
        return None;
    }
    Some(FrameLayout {
        stack_size,
        fp_offset: stack_size - 16,
        lr_offset: stack_size - 8,
    })
}

fn frame_layout_for_function(func: &crate::stackmap::StackMapFunction) -> Option<FrameLayout> {
    if let Some(layout) = decode_frame_layout_from_prologue(func.address) {
        return Some(layout);
    }
    if func.stack_size == u64::MAX {
        return None;
    }
    frame_layout_from_stack_size(func.stack_size as usize)
}

fn decode_frame_layout_from_prologue(func_addr: u64) -> Option<FrameLayout> {
    #[cfg(not(target_arch = "aarch64"))]
    {
        let _ = func_addr;
        return None;
    }

    #[cfg(target_arch = "aarch64")]
    {
        // Pattern A (main): stp x29, x30, [sp, #-16]!; mov x29, sp; sub sp, sp, #imm
        let inst0 = unsafe { *(func_addr as *const u32) };
        let inst1 = unsafe { *((func_addr as *const u32).add(1)) };
        let inst2 = unsafe { *((func_addr as *const u32).add(2)) };
        if inst0 == 0xA9BF_7BFD && inst1 == 0x9100_03FD && (inst2 & 0xFFC0_03FF) == 0xD100_03FF {
            let imm12 = (inst2 >> 10) & 0xFFF;
            let size = imm12 as usize;
            if size == 0 {
                return None;
            }
            return Some(FrameLayout {
                stack_size: size + 16,
                fp_offset: size,
                lr_offset: size + 8,
            });
        }

        // Pattern B (common): sub sp, sp, #imm; stp x29, x30, [sp, #imm-16]; add x29, sp, #imm-16
        if (inst0 & 0xFFC0_03FF) == 0xD100_03FF {
            let imm12 = (inst0 >> 10) & 0xFFF;
            let size = imm12 as usize;
            if size >= 16 && size % 16 == 0 && inst2 == (0x9100_03FD | (((size - 16) as u32) << 10)) {
                let imm7 = (size - 16) / 8;
                let expected_stp = 0xA900_7BFD | ((imm7 as u32) << 15);
                if inst1 == expected_stp {
                    return Some(FrameLayout {
                        stack_size: size,
                        fp_offset: size - 16,
                        lr_offset: size - 8,
                    });
                }
            }
        }

        None
    }
}

fn lookup_safepoint_record(
    safepoint_map: &HashMap<u64, usize>,
    ra: u64,
) -> Option<&usize> {
    const DELTAS: [u64; 4] = [0, 4, 8, 12];
    for delta in DELTAS {
        if ra >= delta {
            if let Some(idx) = safepoint_map.get(&(ra - delta)) {
                return Some(idx);
            }
        }
    }
    None
}

thread_local! {
    static GC: RefCell<Option<GenerationalGC>> = const { RefCell::new(None) };
}

// ============================================================================
// Young Generation (Bump Pointer)
// ============================================================================

struct YoungGeneration {
    start: *mut u8,
    end: *mut u8,
    alloc_ptr: *mut u8,
    size: usize,
}

unsafe impl Send for YoungGeneration {}
unsafe impl Sync for YoungGeneration {}

impl YoungGeneration {
    fn new(size: usize) -> Self {
        let start = unsafe {
            libc::mmap(
                ptr::null_mut(),
                size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                -1,
                0,
            ) as *mut u8
        };
        if start == libc::MAP_FAILED as *mut u8 {
            panic!("Failed to allocate young generation");
        }
        Self {
            start,
            end: unsafe { start.add(size) },
            alloc_ptr: start,
            size,
        }
    }

    fn contains(&self, ptr: *const u8) -> bool {
        let p = ptr as usize;
        let start = self.start as usize;
        let end = self.end as usize;
        p >= start && p < end
    }

    fn can_allocate(&self, size: usize) -> bool {
        let new_ptr = unsafe { self.alloc_ptr.add(size) };
        new_ptr <= self.end
    }

    fn allocate(&mut self, size: usize) -> *mut u8 {
        let ptr = self.alloc_ptr;
        self.alloc_ptr = unsafe { self.alloc_ptr.add(size) };
        ptr
    }

    fn clear(&mut self) {
        self.alloc_ptr = self.start;
    }

    fn bytes_used(&self) -> usize {
        self.alloc_ptr as usize - self.start as usize
    }
}

impl Drop for YoungGeneration {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.start as *mut c_void, self.size);
        }
    }
}

// ============================================================================
// Old Generation (Mark and Sweep with Free List)
// ============================================================================

#[derive(Copy, Clone, Debug)]
struct FreeBlock {
    offset: usize,
    size: usize,
}

impl FreeBlock {
    fn end(&self) -> usize {
        self.offset + self.size
    }

    fn contains(&self, offset: usize) -> bool {
        self.offset <= offset && offset < self.end()
    }
}

struct OldGeneration {
    start: *mut u8,
    page_count: usize,
    highmark: usize,
    free_list: Vec<FreeBlock>,
}

unsafe impl Send for OldGeneration {}
unsafe impl Sync for OldGeneration {}

impl OldGeneration {
    fn new(initial_pages: usize) -> Self {
        let start = unsafe {
            libc::mmap(
                ptr::null_mut(),
                get_page_size() * MAX_OLD_GEN_PAGES,
                libc::PROT_NONE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                -1,
                0,
            ) as *mut u8
        };
        if start == libc::MAP_FAILED as *mut u8 {
            panic!("Failed to reserve old generation");
        }

        // Commit initial pages
        unsafe {
            libc::mprotect(
                start as *mut c_void,
                initial_pages * get_page_size(),
                libc::PROT_READ | libc::PROT_WRITE,
            );
        }

        let size = initial_pages * get_page_size();
        Self {
            start,
            page_count: initial_pages,
            highmark: 0,
            free_list: vec![FreeBlock { offset: 0, size }],
        }
    }

    fn byte_count(&self) -> usize {
        self.page_count * get_page_size()
    }

    fn contains(&self, ptr: *const u8) -> bool {
        let p = ptr as usize;
        let start = self.start as usize;
        let end = start + self.byte_count();
        p >= start && p < end
    }

    fn allocate(&mut self, size: usize) -> Option<*mut u8> {
        // Find a free block that fits
        for (i, block) in self.free_list.iter_mut().enumerate() {
            if block.size >= size {
                let ptr = unsafe { self.start.add(block.offset) };
                let offset = block.offset;

                block.offset += size;
                block.size -= size;

                if block.size == 0 {
                    self.free_list.remove(i);
                }

                if offset + size > self.highmark {
                    self.highmark = offset + size;
                }

                return Some(ptr);
            }
        }
        None
    }

    fn copy_object(&mut self, data: &[u8]) -> *mut u8 {
        let size = data.len();
        loop {
            if let Some(ptr) = self.allocate(size) {
                unsafe {
                    ptr::copy_nonoverlapping(data.as_ptr(), ptr, size);
                }
                return ptr;
            }
            // Need to grow
            self.grow();
        }
    }

    fn grow(&mut self) {
        let old_size = self.byte_count();
        let new_page_count = self.page_count * 2;

        unsafe {
            libc::mprotect(
                self.start as *mut c_void,
                new_page_count * get_page_size(),
                libc::PROT_READ | libc::PROT_WRITE,
            );
        }

        let new_size = new_page_count * get_page_size();
        self.free_list.push(FreeBlock {
            offset: old_size,
            size: new_size - old_size,
        });
        self.page_count = new_page_count;
    }

    fn find_free_block_containing(&self, offset: usize) -> Option<&FreeBlock> {
        self.free_list.iter().find(|b| b.contains(offset))
    }

    fn insert_free_block(&mut self, block: FreeBlock) {
        // Insert and coalesce
        let mut i = match self.free_list.binary_search_by_key(&block.offset, |b| b.offset) {
            Ok(i) | Err(i) => i,
        };

        // Coalesce with previous
        if i > 0 && self.free_list[i - 1].end() == block.offset {
            i -= 1;
            self.free_list[i].size += block.size;
        } else {
            self.free_list.insert(i, block);
        }

        // Coalesce with next
        if i + 1 < self.free_list.len() && self.free_list[i].end() == self.free_list[i + 1].offset {
            self.free_list[i].size += self.free_list[i + 1].size;
            self.free_list.remove(i + 1);
        }
    }
}

// ============================================================================
// Card Table for Write Barriers
// ============================================================================

const CARD_SIZE_LOG2: usize = 9;
const CARD_SIZE: usize = 1 << CARD_SIZE_LOG2;

struct CardTable {
    cards: Vec<u8>,
    heap_start: usize,
    dirty_indices: Vec<usize>,
}

impl CardTable {
    fn new(heap_start: usize, heap_size: usize) -> Self {
        let card_count = heap_size.div_ceil(CARD_SIZE);
        Self {
            cards: vec![0; card_count],
            heap_start,
            dirty_indices: Vec::with_capacity(64),
        }
    }

    fn mark_dirty(&mut self, addr: usize) {
        if addr < self.heap_start {
            return;
        }
        let card_idx = (addr - self.heap_start) >> CARD_SIZE_LOG2;
        if card_idx < self.cards.len() && self.cards[card_idx] == 0 {
            self.cards[card_idx] = 1;
            self.dirty_indices.push(card_idx);
        }
    }

    fn resize(&mut self, new_heap_size: usize) {
        let new_count = new_heap_size.div_ceil(CARD_SIZE);
        if new_count > self.cards.len() {
            self.cards.resize(new_count, 0);
        }
    }

    fn clear(&mut self) {
        for &idx in &self.dirty_indices {
            self.cards[idx] = 0;
        }
        self.dirty_indices.clear();
    }

    fn dirty_cards(&self) -> &[usize] {
        &self.dirty_indices
    }
}

// ============================================================================
// Generational GC
// ============================================================================

pub struct GenerationalGC {
    young: YoungGeneration,
    old: OldGeneration,
    card_table: CardTable,
    remembered_set: Vec<TaggedValue>,
    gc_count: usize,
    minor_gc_count: usize,
    full_gc_frequency: usize,
    global_root: TaggedValue,
    verbose: bool,
    bytes_promoted: usize,
}

impl GenerationalGC {
    pub fn new() -> Self {
        let young_size = young_gen_size();
        let old_pages = old_gen_initial_pages();

        let young = YoungGeneration::new(young_size);
        let old = OldGeneration::new(old_pages);
        let card_table = CardTable::new(old.start as usize, old.byte_count());

        eprintln!(
            "Generational GC initialized: {}MB young, {}MB old (initial)",
            young_size / 1024 / 1024,
            old.byte_count() / 1024 / 1024
        );

        Self {
            young,
            old,
            card_table,
            remembered_set: Vec::with_capacity(64),
            gc_count: 0,
            minor_gc_count: 0,
            full_gc_frequency: 50,
            global_root: 0,
            verbose: std::env::var("GC_TRACE").is_ok(),
            bytes_promoted: 0,
        }
    }

    pub fn has_space_for_cons(&self) -> bool {
        let size = HEAP_HEADER_SIZE + std::mem::size_of::<ConsCell>();
        let aligned = (size + 7) & !7;
        self.young.can_allocate(aligned)
    }

    pub fn alloc_cons_no_gc(&mut self, car: TaggedValue, cdr: TaggedValue) -> *mut u8 {
        let size = HEAP_HEADER_SIZE + std::mem::size_of::<ConsCell>();
        let aligned = (size + 7) & !7;

        let header_ptr = self.young.allocate(aligned) as *mut HeapObjectHeader;
        unsafe {
            (*header_ptr).forwarding = ptr::null_mut();
            (*header_ptr).size = (aligned - HEAP_HEADER_SIZE) as u32;
            (*header_ptr).magic = HEAP_MAGIC;
            (*header_ptr).obj_type = HeapObjectType::Cons;
            (*header_ptr).ptr_count = 2;
            (*header_ptr).flags = 0;
        }

        let data_ptr = unsafe { (header_ptr as *mut u8).add(HEAP_HEADER_SIZE) };
        unsafe {
            let cons = data_ptr as *mut ConsCell;
            (*cons).car = car;
            (*cons).cdr = cdr;
        }

        data_ptr
    }

    pub fn write_barrier(&mut self, object_ptr: TaggedValue, new_value: TaggedValue) {
        // Only care about heap pointer values pointing to young gen
        if !is_heap_ptr(new_value) {
            return;
        }
        if !self.young.contains(new_value as *const u8) {
            return;
        }

        // Only care if object is in old gen
        if !is_heap_ptr(object_ptr) {
            return;
        }
        if !self.old.contains(object_ptr as *const u8) {
            return;
        }

        // Mark card and add to remembered set
        self.card_table.mark_dirty(object_ptr as usize);
        if !self.remembered_set.contains(&object_ptr) {
            self.remembered_set.push(object_ptr);
        }
    }

    pub fn collect_with_stack_walk(&mut self, fp: usize, sp: usize, ra: usize) {
        self.gc_count += 1;
        self.minor_gc_count += 1;

        // Periodically do a full GC
        if self.minor_gc_count % self.full_gc_frequency == 0 {
            self.full_gc(fp, sp, ra);
        } else {
            self.minor_gc(fp, sp, ra);
        }
    }

    fn minor_gc(&mut self, fp: usize, sp: usize, ra: usize) {
        let start = std::time::Instant::now();
        self.bytes_promoted = 0;
        let objects_promoted = std::cell::Cell::new(0usize);

        if self.verbose {
            eprintln!(
                "[GC] Minor collection #{}, young gen used: {} bytes",
                self.gc_count,
                self.young.bytes_used()
            );
            eprintln!(
                "[GC] global_root={:#x}, in_young={}, in_old={}",
                self.global_root,
                self.young.contains(self.global_root as *const u8),
                self.old.contains(self.global_root as *const u8)
            );
        }

        // Process global root
        if is_heap_ptr(self.global_root) && self.young.contains(self.global_root as *const u8) {
            let old_root = self.global_root;
            self.global_root = self.copy_to_old(self.global_root);
            if self.verbose {
                eprintln!("[GC] Promoted global_root from {:#x} to {:#x}", old_root, self.global_root);
                // Verify tree structure
                let depth = self.count_tree_depth(self.global_root);
                let count = self.count_tree_nodes(self.global_root);
                eprintln!("[GC] After promotion: tree depth={}, nodes={}", depth, count);
            }
        } else if is_heap_ptr(self.global_root) && self.old.contains(self.global_root as *const u8) {
            // Global root is already in old gen - no need to scan entire tree.
            // Any old->young pointers are tracked via card marking / remembered set.
            if self.verbose {
                eprintln!("[GC] global_root already in old gen: {:#x}", self.global_root);
            }
        }

        let _ = objects_promoted;

        // Walk stack and promote young gen roots
        self.walk_stack_and_promote(fp, sp, ra);

        // Process remembered set
        let remembered = std::mem::take(&mut self.remembered_set);
        for old_obj in remembered {
            self.scan_old_object(old_obj);
        }

        // Process dirty cards
        self.process_dirty_cards();

        // Clear young generation
        self.young.clear();
        self.card_table.clear();

        if self.verbose {
            eprintln!(
                "[GC] Minor GC complete: {} bytes promoted in {:?}",
                self.bytes_promoted,
                start.elapsed()
            );
            // Verify tree after GC completes
            if is_heap_ptr(self.global_root) {
                let count = self.count_tree_nodes(self.global_root);
                eprintln!("[GC] After full GC: global_root nodes={}", count);
            }
        }
    }

    fn full_gc(&mut self, fp: usize, sp: usize, ra: usize) {
        let start = std::time::Instant::now();

        if self.verbose {
            eprintln!("[GC] Full collection #{}", self.gc_count);
        }

        // First do a minor GC to promote everything
        self.bytes_promoted = 0;

        // Process global root
        if is_heap_ptr(self.global_root) {
            if self.young.contains(self.global_root as *const u8) {
                self.global_root = self.copy_to_old(self.global_root);
            }
            // Mark old gen root
            self.mark_object(self.global_root);
        }

        // Walk stack
        self.walk_stack_and_promote(fp, sp, ra);

        // Process remembered set
        let remembered = std::mem::take(&mut self.remembered_set);
        for old_obj in remembered {
            self.scan_old_object(old_obj);
        }
        self.process_dirty_cards();

        // Now sweep old generation
        self.sweep_old();

        self.young.clear();
        self.card_table.clear();

        if self.verbose {
            eprintln!("[GC] Full GC complete in {:?}", start.elapsed());
        }
    }

    fn walk_stack_and_promote(&mut self, fp: usize, sp: usize, ra: usize) {
        let stack_low = STACK_LOW.load(Ordering::Acquire);
        let stack_high = STACK_HIGH.load(Ordering::Acquire);

        STACKMAP.with(|sm_cell| {
            SAFEPOINT_MAP.with(|sp_map_cell| {
                let sm_ref = sm_cell.borrow();
                let sp_map = sp_map_cell.borrow();

                if let Some(ref stackmap) = *sm_ref {
                    // Manual stack walk like MMTk does
                    let mut current_ra = ra as u64;
                    let mut current_sp = sp;
                    let mut current_fp = fp;
                    let mut frame_depth = 0usize;

                    loop {
                        frame_depth += 1;
                        if frame_depth > 100 {
                            break;
                        }

                        // Find function for this PC
                        let func = match function_for_pc(stackmap, current_ra) {
                            Some(f) => f,
                            None => break,
                        };

                        // Get frame layout
                        let layout = match frame_layout_for_function(func) {
                            Some(l) => l,
                            None => break,
                        };

                        // Fix up FP/SP if needed
                        if current_fp == 0 {
                            current_fp = current_sp + layout.fp_offset;
                        }
                        if current_sp < stack_low || current_sp > stack_high {
                            break;
                        }

                        // Look up safepoint record for precise GC locations
                        if let Some(&record_idx) = lookup_safepoint_record(&sp_map, current_ra) {
                            let record = &stackmap.records[record_idx];
                            let gc_locs = stackmap.get_gc_locations(record);

                            if self.verbose {
                                eprintln!("[GC] Frame {}: ra={:#x} sp={:#x} fp={:#x}, {} GC locs",
                                    frame_depth, current_ra, current_sp, current_fp, gc_locs.len());
                            }

                            // Process each GC location precisely
                            for (_base_loc, derived_loc) in gc_locs.iter() {
                                if let Some(addr) = resolve_location(
                                    derived_loc,
                                    current_fp,
                                    current_sp,
                                    stack_low,
                                    stack_high,
                                ) {
                                    if addr & 7 != 0 {
                                        if self.verbose {
                                            eprintln!("[GC]   loc reg={} off={} -> addr {:#x} MISALIGNED",
                                                derived_loc.reg, derived_loc.offset, addr);
                                        }
                                        continue;
                                    }
                                    let slot = addr as *mut TaggedValue;
                                    let val = unsafe { *slot };

                                    if is_heap_ptr(val) {
                                        if self.young.contains(val as *const u8) {
                                            // Promote young gen pointer
                                            let new_val = self.copy_to_old(val);
                                            if self.verbose {
                                                eprintln!("[GC]   slot {:#x}: {:#x} -> {:#x}", addr, val, new_val);
                                            }
                                            unsafe { *slot = new_val };
                                        } else if self.old.contains(val as *const u8) {
                                            // Mark old gen pointer (needed for full GC)
                                            self.mark_object(val);
                                            if self.verbose {
                                                eprintln!("[GC]   slot {:#x}: marked old gen {:#x}", addr, val);
                                            }
                                        }
                                    } else if self.verbose {
                                        eprintln!("[GC]   slot {:#x}: val={:#x} not heap ptr", addr, val);
                                    }
                                } else if self.verbose {
                                    eprintln!("[GC]   loc reg={} off={} type={:?} -> UNRESOLVED",
                                        derived_loc.reg, derived_loc.offset, derived_loc.ty);
                                }
                            }
                        }

                        // Walk to caller frame
                        let lr_slot = match current_sp.checked_add(layout.lr_offset) {
                            Some(val) => val,
                            None => break,
                        };
                        if lr_slot + 8 > stack_high {
                            break;
                        }

                        let caller_ra = unsafe { std::ptr::read_unaligned(lr_slot as *const usize) } as u64;
                        if function_for_pc(stackmap, caller_ra).is_none() {
                            break;
                        }

                        let caller_sp = match current_sp.checked_add(layout.stack_size) {
                            Some(val) => val,
                            None => break,
                        };
                        if caller_sp < stack_low || caller_sp > stack_high {
                            break;
                        }

                        current_ra = caller_ra;
                        current_sp = caller_sp;
                        current_fp = 0; // Will be recalculated
                    }
                }
            });
        });
    }

    fn copy_to_old(&mut self, val: TaggedValue) -> TaggedValue {
        if !is_heap_ptr(val) || !self.young.contains(val as *const u8) {
            return val;
        }

        // Use iterative approach with worklist to avoid stack overflow on deep trees
        let mut worklist: Vec<*mut TaggedValue> = Vec::new();

        // Copy the root object first
        let result = self.copy_single_object(val);

        // Add its fields to the worklist
        self.add_fields_to_worklist(result, &mut worklist);

        // Process worklist iteratively
        while let Some(slot_ptr) = worklist.pop() {
            let slot_val = unsafe { *slot_ptr };
            if is_heap_ptr(slot_val) && self.young.contains(slot_val as *const u8) {
                let new_val = self.copy_single_object(slot_val);
                unsafe { *slot_ptr = new_val };
                self.add_fields_to_worklist(new_val, &mut worklist);
            }
        }

        result
    }

    /// Copy a single object to old gen without recursing into children
    fn copy_single_object(&mut self, val: TaggedValue) -> TaggedValue {
        let data_ptr = val as *mut u8;
        let header_ptr = unsafe { data_ptr.sub(HEAP_HEADER_SIZE) as *mut HeapObjectHeader };
        let header = unsafe { &mut *header_ptr };

        // Check forwarding pointer
        if !header.forwarding.is_null() {
            return header.forwarding as TaggedValue;
        }

        // Copy to old generation
        let total_size = HEAP_HEADER_SIZE + header.size as usize;
        let old_data = unsafe { std::slice::from_raw_parts(header_ptr as *const u8, total_size) };
        let new_header_ptr = self.old.copy_object(old_data);
        let new_data_ptr = unsafe { new_header_ptr.add(HEAP_HEADER_SIZE) };

        // Clear forwarding in new copy
        unsafe {
            (*(new_header_ptr as *mut HeapObjectHeader)).forwarding = ptr::null_mut();
        }

        // Set forwarding pointer in old location
        header.forwarding = new_data_ptr;

        self.bytes_promoted += total_size;
        new_data_ptr as TaggedValue
    }

    /// Add child fields of a newly copied object to the worklist
    fn add_fields_to_worklist(&self, val: TaggedValue, worklist: &mut Vec<*mut TaggedValue>) {
        if !is_heap_ptr(val) {
            return;
        }
        let data_ptr = val as *mut u8;
        let header_ptr = unsafe { data_ptr.sub(HEAP_HEADER_SIZE) as *const HeapObjectHeader };
        let header = unsafe { &*header_ptr };

        if header.obj_type == HeapObjectType::Cons {
            let cons = data_ptr as *mut ConsCell;
            unsafe {
                worklist.push(&mut (*cons).car as *mut TaggedValue);
                worklist.push(&mut (*cons).cdr as *mut TaggedValue);
            }
        }
    }

    fn scan_old_object(&mut self, obj: TaggedValue) {
        if !is_heap_ptr(obj) {
            return;
        }
        let data_ptr = obj as *mut u8;
        let header_ptr = unsafe { data_ptr.sub(HEAP_HEADER_SIZE) as *const HeapObjectHeader };
        let header = unsafe { &*header_ptr };

        if header.obj_type == HeapObjectType::Cons {
            let cons = data_ptr as *mut ConsCell;
            unsafe {
                if is_heap_ptr((*cons).car) && self.young.contains((*cons).car as *const u8) {
                    (*cons).car = self.copy_to_old((*cons).car);
                }
                if is_heap_ptr((*cons).cdr) && self.young.contains((*cons).cdr as *const u8) {
                    (*cons).cdr = self.copy_to_old((*cons).cdr);
                }
            }
        }
    }

    fn count_tree_depth(&self, obj: TaggedValue) -> usize {
        if !is_heap_ptr(obj) {
            return 0;
        }
        let data_ptr = obj as *const u8;
        let header_ptr = unsafe { data_ptr.sub(HEAP_HEADER_SIZE) as *const HeapObjectHeader };
        let header = unsafe { &*header_ptr };

        if header.obj_type != HeapObjectType::Cons {
            return 0;
        }

        let cons = data_ptr as *const ConsCell;
        let car = unsafe { (*cons).car };
        let cdr = unsafe { (*cons).cdr };

        if !is_heap_ptr(car) && !is_heap_ptr(cdr) {
            return 1; // Leaf node
        }

        let car_depth = self.count_tree_depth(car);
        let cdr_depth = self.count_tree_depth(cdr);
        1 + car_depth.max(cdr_depth)
    }

    fn count_tree_nodes(&self, obj: TaggedValue) -> usize {
        if !is_heap_ptr(obj) {
            return 0;
        }
        let data_ptr = obj as *const u8;
        let header_ptr = unsafe { data_ptr.sub(HEAP_HEADER_SIZE) as *const HeapObjectHeader };
        let header = unsafe { &*header_ptr };

        if header.obj_type != HeapObjectType::Cons {
            return 0;
        }

        let cons = data_ptr as *const ConsCell;
        let car = unsafe { (*cons).car };
        let cdr = unsafe { (*cons).cdr };

        1 + self.count_tree_nodes(car) + self.count_tree_nodes(cdr)
    }

    /// Iteratively scan an old gen object tree for young gen references
    fn scan_old_object_recursive(&mut self, obj: TaggedValue) {
        // Use iterative approach with worklist to avoid stack overflow
        let mut worklist = vec![obj];

        while let Some(current) = worklist.pop() {
            if !is_heap_ptr(current) {
                continue;
            }

            // Only scan objects in old gen
            if !self.old.contains(current as *const u8) {
                continue;
            }

            let data_ptr = current as *mut u8;
            let header_ptr = unsafe { data_ptr.sub(HEAP_HEADER_SIZE) as *const HeapObjectHeader };
            let header = unsafe { &*header_ptr };

            if header.obj_type == HeapObjectType::Cons {
                let cons = data_ptr as *mut ConsCell;
                unsafe {
                    // Check car
                    if is_heap_ptr((*cons).car) {
                        if self.young.contains((*cons).car as *const u8) {
                            (*cons).car = self.copy_to_old((*cons).car);
                        } else if self.old.contains((*cons).car as *const u8) {
                            worklist.push((*cons).car);
                        }
                    }
                    // Check cdr
                    if is_heap_ptr((*cons).cdr) {
                        if self.young.contains((*cons).cdr as *const u8) {
                            (*cons).cdr = self.copy_to_old((*cons).cdr);
                        } else if self.old.contains((*cons).cdr as *const u8) {
                            worklist.push((*cons).cdr);
                        }
                    }
                }
            }
        }
    }

    fn process_dirty_cards(&mut self) {
        let dirty_cards: std::collections::HashSet<usize> =
            self.card_table.dirty_cards().iter().copied().collect();

        if dirty_cards.is_empty() {
            return;
        }

        let old_start = self.old.start as usize;
        let mut offset = 0;

        while offset < self.old.highmark {
            if let Some(free) = self.old.find_free_block_containing(offset) {
                offset = free.end();
                continue;
            }

            let card_idx = (old_start + offset - self.card_table.heap_start) >> CARD_SIZE_LOG2;
            if dirty_cards.contains(&card_idx) {
                let header_ptr = unsafe { self.old.start.add(offset) as *const HeapObjectHeader };
                let header = unsafe { &*header_ptr };

                if header.magic == HEAP_MAGIC {
                    let data_ptr = unsafe { (header_ptr as *const u8).add(HEAP_HEADER_SIZE) };
                    self.scan_old_object(data_ptr as TaggedValue);
                    offset += HEAP_HEADER_SIZE + header.size as usize;
                    offset = (offset + 7) & !7;
                    continue;
                }
            }

            // Skip to next potential object
            let header_ptr = unsafe { self.old.start.add(offset) as *const HeapObjectHeader };
            let header = unsafe { &*header_ptr };
            if header.magic == HEAP_MAGIC {
                offset += HEAP_HEADER_SIZE + header.size as usize;
                offset = (offset + 7) & !7;
            } else {
                offset += 8;
            }
        }
    }

    fn mark_object(&mut self, obj: TaggedValue) {
        // Use iterative approach with worklist to avoid stack overflow
        let mut worklist = vec![obj];

        while let Some(current) = worklist.pop() {
            if !is_heap_ptr(current) {
                continue;
            }
            let data_ptr = current as *mut u8;
            let header_ptr = unsafe { data_ptr.sub(HEAP_HEADER_SIZE) as *mut HeapObjectHeader };
            let header = unsafe { &mut *header_ptr };

            if header.flags & 1 != 0 {
                continue; // Already marked
            }
            header.flags |= 1; // Set mark bit

            if header.obj_type == HeapObjectType::Cons {
                let cons = data_ptr as *const ConsCell;
                unsafe {
                    worklist.push((*cons).car);
                    worklist.push((*cons).cdr);
                }
            }
        }
    }

    fn sweep_old(&mut self) {
        let mut offset = 0;
        let mut new_free_list = Vec::new();

        while offset < self.old.highmark {
            if let Some(free) = self.old.find_free_block_containing(offset) {
                new_free_list.push(*free);
                offset = free.end();
                continue;
            }

            let header_ptr = unsafe { self.old.start.add(offset) as *mut HeapObjectHeader };
            let header = unsafe { &mut *header_ptr };

            if header.magic != HEAP_MAGIC {
                offset += 8;
                continue;
            }

            let obj_size = HEAP_HEADER_SIZE + header.size as usize;
            let aligned_size = (obj_size + 7) & !7;

            if header.flags & 1 != 0 {
                // Marked - keep it, clear mark
                header.flags &= !1;
            } else {
                // Unmarked - add to free list
                new_free_list.push(FreeBlock {
                    offset,
                    size: aligned_size,
                });
            }

            offset += aligned_size;
        }

        // Add remaining space
        if offset < self.old.byte_count() {
            new_free_list.push(FreeBlock {
                offset,
                size: self.old.byte_count() - offset,
            });
        }

        // Coalesce free blocks
        new_free_list.sort_by_key(|b| b.offset);
        self.old.free_list.clear();
        for block in new_free_list {
            self.old.insert_free_block(block);
        }
    }

    pub fn set_global_root(&mut self, val: TaggedValue) {
        self.global_root = val;
    }

    pub fn clear_global_root(&mut self) {
        self.global_root = 0;
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

fn resolve_location(
    loc: &Location,
    fp: usize,
    sp: usize,
    stack_low: usize,
    stack_high: usize,
) -> Option<usize> {
    if loc.ty == LocationType::Register {
        return None;
    }

    #[cfg(target_arch = "aarch64")]
    let base = match loc.reg {
        29 => fp,  // Frame pointer
        31 => sp,  // Stack pointer
        _ => {
            // For other registers, we'd need the saved register context.
            // Fall back to SP-relative if we don't have it.
            // This handles cases like x19-x28 (callee-saved) which might be used.
            sp
        }
    };
    #[cfg(target_arch = "x86_64")]
    let base = match loc.reg {
        6 => fp,   // RBP
        7 => sp,   // RSP
        _ => sp,   // Fall back to SP
    };
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    let base = sp;

    let addr = if loc.offset >= 0 {
        base.checked_add(loc.offset as usize)?
    } else {
        base.checked_sub((-loc.offset) as usize)?
    };

    if addr < stack_low || addr + 8 > stack_high {
        return None;
    }

    match loc.ty {
        LocationType::Direct | LocationType::Indirect => Some(addr),
        _ => None,
    }
}

// ============================================================================
// Public API
// ============================================================================

pub fn init() {
    GC.with(|gc| {
        *gc.borrow_mut() = Some(GenerationalGC::new());
    });
}

pub fn load_stackmaps(stackmap: StackMap, _main_fn: usize) {
    let mut safepoint_map = HashMap::new();
    for (idx, record) in stackmap.records.iter().enumerate() {
        safepoint_map.insert(record.absolute_offset, idx);
    }

    SAFEPOINT_MAP.with(|map| {
        *map.borrow_mut() = safepoint_map;
    });

    STACKMAP.with(|sm| {
        *sm.borrow_mut() = Some(stackmap);
    });

    #[cfg(target_os = "macos")]
    {
        let pthread = unsafe { libc::pthread_self() };
        let stack_size = unsafe { libc::pthread_get_stacksize_np(pthread) };
        let stack_high = unsafe { libc::pthread_get_stackaddr_np(pthread) } as usize;
        let stack_low = stack_high.saturating_sub(stack_size);
        STACK_LOW.store(stack_low, Ordering::Release);
        STACK_HIGH.store(stack_high, Ordering::Release);
    }
    #[cfg(not(target_os = "macos"))]
    {
        STACK_LOW.store(0, Ordering::Release);
        STACK_HIGH.store(usize::MAX, Ordering::Release);
    }
}

// ============================================================================
// Runtime Functions
// ============================================================================

#[no_mangle]
pub extern "C" fn rt_gen_try_alloc_cons(car: TaggedValue, cdr: TaggedValue) -> *mut u8 {
    GC.with(|gc| {
        let mut gc = gc.borrow_mut();
        let collector = gc.as_mut().expect("GC not initialized");

        if !collector.has_space_for_cons() {
            return ptr::null_mut();
        }

        collector.alloc_cons_no_gc(car, cdr)
    })
}

/// Safepoint-based cons allocation for generational GC.
/// Takes SLOT ADDRESSES (not values) so that if GC triggers during allocation,
/// the slots can be updated. Reads car/cdr from slots AFTER allocation.
static CONS_SAFEPOINT_GC_COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[no_mangle]
pub extern "C" fn rt_gen_cons_safepoint(
    car_slot: *mut TaggedValue,
    cdr_slot: *mut TaggedValue,
    fp: usize,
    sp: usize,
    ra: usize,
) -> *mut u8 {
    GC.with(|gc| {
        let mut gc = gc.borrow_mut();
        let collector = gc.as_mut().expect("GC not initialized");

        // Read values BEFORE GC for debugging
        let car_before = unsafe { *car_slot };
        let cdr_before = unsafe { *cdr_slot };

        // Try to allocate, triggering GC if needed
        let mut gc_triggered = false;
        while !collector.has_space_for_cons() {
            gc_triggered = true;
            collector.collect_with_stack_walk(fp, sp, ra);
        }

        // Read car/cdr from slots AFTER any GC (values may have been relocated)
        let car = unsafe { *car_slot };
        let cdr = unsafe { *cdr_slot };

        // Debug: if GC was triggered, check if values were updated
        if gc_triggered && collector.verbose {
            let count = CONS_SAFEPOINT_GC_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if count < 20 {
                eprintln!("[CONS_SAFEPOINT] GC triggered, car_slot={:#x}, cdr_slot={:#x}",
                    car_slot as usize, cdr_slot as usize);
                eprintln!("[CONS_SAFEPOINT] car: {:#x} -> {:#x}, cdr: {:#x} -> {:#x}",
                    car_before, car, cdr_before, cdr);
                if car_before != car || cdr_before != cdr {
                    eprintln!("[CONS_SAFEPOINT] Values were UPDATED by GC!");
                } else {
                    eprintln!("[CONS_SAFEPOINT] Values were NOT updated");
                }
            }
        }

        collector.alloc_cons_no_gc(car, cdr)
    })
}

#[no_mangle]
pub extern "C" fn rt_gen_gc_with_frame_info(fp: usize, sp: usize, ra: usize) {
    GC.with(|gc| {
        if let Some(ref mut collector) = *gc.borrow_mut() {
            if collector.verbose {
                eprintln!("[GC] rt_gen_gc_with_frame_info called, fp={:#x}", fp);
            }
            collector.collect_with_stack_walk(fp, sp, ra);
        }
    });
}

/// GC function that takes gc pointer values as arguments.
/// This forces LLVM to include them in gc-live, so the statepoint rewriter
/// will spill them to stack slots and record their locations in the stackmap.
/// After this call, use gc.relocate to get the updated values.
///
/// The dummy_car and dummy_cdr parameters are just to force them into gc-live.
/// Their values are not used by this function.
#[no_mangle]
pub extern "C" fn rt_gen_gc_with_roots(
    fp: usize,
    sp: usize,
    ra: usize,
    _dummy_car: TaggedValue,  // Forces LLVM to track this in gc-live
    _dummy_cdr: TaggedValue,  // Forces LLVM to track this in gc-live
) {
    GC.with(|gc| {
        if let Some(ref mut collector) = *gc.borrow_mut() {
            if collector.verbose {
                eprintln!("[GC] rt_gen_gc_with_roots called, fp={:#x}", fp);
            }
            collector.collect_with_stack_walk(fp, sp, ra);
        }
    });
}

#[no_mangle]
pub extern "C" fn rt_gen_gc_simple() {
    // Can't do much without frame info
    eprintln!("[GC] Warning: rt_gen_gc_simple called without frame info");
}

static CAR_CALL_COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static LAST_TRACED_GC_COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[no_mangle]
pub extern "C" fn rt_gen_car(cell: TaggedValue) -> TaggedValue {
    let count = CAR_CALL_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let ptr = cell as *const ConsCell;
    let result = unsafe { (*ptr).car };

    // Debug: check if the cell is in young gen when global_root is in old gen
    // This would indicate the wrong tree is being traversed
    GC.with(|gc| {
        if let Some(ref collector) = *gc.borrow() {
            // Only trace once per GC cycle to avoid spam
            let gc_count = collector.gc_count;
            let last_traced = LAST_TRACED_GC_COUNT.load(std::sync::atomic::Ordering::Relaxed);

            if collector.verbose {
                // Check if global_root is in old gen but cell is in young gen
                let global_in_old = is_heap_ptr(collector.global_root) &&
                    collector.old.contains(collector.global_root as *const u8);
                let cell_in_young = collector.young.contains(cell as *const u8);

                if global_in_old && cell_in_young && gc_count != last_traced {
                    LAST_TRACED_GC_COUNT.store(gc_count, std::sync::atomic::Ordering::Relaxed);
                    eprintln!(
                        "[GC DEBUG] rt_gen_car: global_root={:#x} in old gen, but cell={:#x} is in YOUNG gen!",
                        collector.global_root, cell
                    );
                    eprintln!(
                        "[GC DEBUG] This suggests the wrong tree is being traversed. gc_count={}",
                        gc_count
                    );
                }
            }
        }
    });

    result
}

#[no_mangle]
pub extern "C" fn rt_gen_cdr(cell: TaggedValue) -> TaggedValue {
    let ptr = cell as *const ConsCell;
    unsafe { (*ptr).cdr }
}

#[no_mangle]
pub extern "C" fn rt_gen_set_global_root(val: TaggedValue) {
    GC.with(|gc| {
        if let Some(ref mut collector) = *gc.borrow_mut() {
            if collector.verbose {
                eprintln!("[GC] set_global_root({:#x})", val);
            }
            collector.set_global_root(val);
        }
    });
}

#[no_mangle]
pub extern "C" fn rt_gen_clear_global_root() {
    GC.with(|gc| {
        if let Some(ref mut collector) = *gc.borrow_mut() {
            if collector.verbose {
                eprintln!("[GC] clear_global_root()");
            }
            collector.clear_global_root();
        }
    });
}

/// Debug function to verify a tree pointer before item_check
#[no_mangle]
pub extern "C" fn rt_gen_verify_tree(tree: TaggedValue) {
    GC.with(|gc| {
        if let Some(ref collector) = *gc.borrow() {
            let in_young = collector.young.contains(tree as *const u8);
            let in_old = collector.old.contains(tree as *const u8);
            let global_root = collector.global_root;
            let global_in_old = collector.old.contains(global_root as *const u8);

            eprintln!("[DEBUG] verify_tree: tree={:#x}, in_young={}, in_old={}", tree, in_young, in_old);
            eprintln!("[DEBUG] global_root={:#x}, global_in_old={}", global_root, global_in_old);

            if tree != global_root {
                eprintln!("[DEBUG] WARNING: tree != global_root!");
            }

            // Count nodes in the passed tree
            let count = collector.count_tree_nodes(tree);
            eprintln!("[DEBUG] Tree node count: {}", count);
        }
    });
}

/// Debug: get global root node count
pub fn debug_get_global_root_node_count() -> usize {
    GC.with(|gc| {
        if let Some(ref collector) = *gc.borrow() {
            collector.count_tree_nodes(collector.global_root)
        } else {
            0
        }
    })
}

pub struct GenerationalRuntimeSymbols {
    pub try_alloc_cons: usize,
    pub cons_safepoint: usize,  // Safepoint-based cons that reads from slots AFTER GC
    pub gc: usize,
    pub gc_with_frame_info: usize,
    pub gc_with_roots: usize,   // GC that takes gc pointers to force them into gc-live
    pub car: usize,
    pub cdr: usize,
    pub set_global_root: usize,
    pub clear_global_root: usize,
    pub verify_tree: usize,
}

pub fn get_generational_runtime_symbols() -> GenerationalRuntimeSymbols {
    GenerationalRuntimeSymbols {
        try_alloc_cons: rt_gen_try_alloc_cons as *const () as usize,
        cons_safepoint: rt_gen_cons_safepoint as *const () as usize,
        gc: rt_gen_gc_simple as *const () as usize,
        gc_with_frame_info: rt_gen_gc_with_frame_info as *const () as usize,
        gc_with_roots: rt_gen_gc_with_roots as *const () as usize,
        car: rt_gen_car as *const () as usize,
        cdr: rt_gen_cdr as *const () as usize,
        set_global_root: rt_gen_set_global_root as *const () as usize,
        clear_global_root: rt_gen_clear_global_root as *const () as usize,
        verify_tree: rt_gen_verify_tree as *const () as usize,
    }
}
