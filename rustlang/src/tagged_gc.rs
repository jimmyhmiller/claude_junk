//! Garbage collector for tagged pointer runtime
//!
//! This GC understands tagged values and only traces actual heap pointers.
//! It scans object interiors based on the object type and ptr_count in the header.
//!
//! Root scanning uses the shadow stack - we walk the shadow stack frames and
//! filter each slot by its tag bits to determine if it's a heap pointer.

use crate::tagged_value::*;
use crate::shadow_stack::{shadow_stack, ShadowFrame};
use std::alloc::{alloc, dealloc, Layout};
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// Size of each semi-space (4MB)
const SEMI_SPACE_SIZE: usize = 4 * 1024 * 1024;

/// The global GC state for tagged values
pub struct TaggedGcState {
    /// The two semi-spaces
    space_a: *mut u8,
    space_b: *mut u8,

    /// Current from-space (where we allocate)
    from_space: *mut u8,
    /// Current to-space (where we copy during GC)
    to_space: *mut u8,

    /// Allocation pointer in from-space
    alloc_ptr: AtomicUsize,
    /// End of from-space
    from_space_end: usize,

    /// Statistics
    pub gc_count: AtomicUsize,
    pub alloc_count: AtomicUsize,
    pub bytes_allocated: AtomicUsize,
    pub move_count: AtomicUsize,
    pub roots_scanned: AtomicUsize,

    /// Force GC on next allocation (for testing)
    pub force_gc: AtomicBool,

    /// Verbose logging
    pub verbose: bool,
}

impl TaggedGcState {
    pub fn new() -> Self {
        unsafe {
            let layout = Layout::from_size_align(SEMI_SPACE_SIZE, 4096).unwrap();
            let space_a = alloc(layout);
            let space_b = alloc(layout);

            if space_a.is_null() || space_b.is_null() {
                panic!("Failed to allocate GC heap");
            }

            // Zero out the spaces
            ptr::write_bytes(space_a, 0, SEMI_SPACE_SIZE);
            ptr::write_bytes(space_b, 0, SEMI_SPACE_SIZE);

            println!("[GC] Initialized with two {}KB semi-spaces", SEMI_SPACE_SIZE / 1024);
            println!("[GC] Space A: {:p} - {:p}", space_a, space_a.add(SEMI_SPACE_SIZE));
            println!("[GC] Space B: {:p} - {:p}", space_b, space_b.add(SEMI_SPACE_SIZE));

            TaggedGcState {
                space_a,
                space_b,
                from_space: space_a,
                to_space: space_b,
                alloc_ptr: AtomicUsize::new(space_a as usize),
                from_space_end: space_a as usize + SEMI_SPACE_SIZE,
                gc_count: AtomicUsize::new(0),
                alloc_count: AtomicUsize::new(0),
                bytes_allocated: AtomicUsize::new(0),
                move_count: AtomicUsize::new(0),
                roots_scanned: AtomicUsize::new(0),
                force_gc: AtomicBool::new(false),
                verbose: true,
            }
        }
    }

    /// Check if a tagged value points into from-space
    pub fn is_from_space_ptr(&self, v: TaggedValue) -> bool {
        if !is_heap_ptr(v) {
            return false;
        }
        let addr = v as usize;
        let from_start = self.from_space as usize;
        let from_end = from_start + SEMI_SPACE_SIZE;
        addr >= from_start && addr < from_end
    }

    /// Check if a tagged value points into to-space (already copied)
    pub fn is_to_space_ptr(&self, v: TaggedValue) -> bool {
        if !is_heap_ptr(v) {
            return false;
        }
        let addr = v as usize;
        let to_start = self.to_space as usize;
        let to_end = to_start + SEMI_SPACE_SIZE;
        addr >= to_start && addr < to_end
    }

    /// Check if we need to collect (allocation would fail)
    fn needs_collection(&self, size: usize) -> bool {
        let current = self.alloc_ptr.load(Ordering::Relaxed);
        let needed = current + HEAP_HEADER_SIZE + ((size + 7) & !7);
        needed > self.from_space_end || self.force_gc.load(Ordering::Relaxed)
    }

    /// Allocate a cons cell
    pub fn alloc_cons(&mut self, car: TaggedValue, cdr: TaggedValue) -> TaggedValue {
        let size = std::mem::size_of::<ConsCell>();

        // Check if we need GC
        if self.needs_collection(size) {
            self.collect();
        }

        let ptr = self.alloc_raw(size, HeapObjectType::Cons, 2);

        unsafe {
            let cons = ptr as *mut ConsCell;
            (*cons).car = car;
            (*cons).cdr = cdr;
        }

        ptr_to_tagged(ptr)
    }

    /// Allocate a vector of tagged values
    pub fn alloc_vector(&mut self, len: usize) -> TaggedValue {
        let size = len * std::mem::size_of::<TaggedValue>();

        if self.needs_collection(size) {
            self.collect();
        }

        let ptr = self.alloc_raw(size, HeapObjectType::Vector, len as u8);

        // Initialize all slots to nil
        unsafe {
            let slots = ptr as *mut TaggedValue;
            for i in 0..len {
                *slots.add(i) = NIL;
            }
        }

        ptr_to_tagged(ptr)
    }

    /// Allocate a byte array (no pointers inside)
    pub fn alloc_bytes(&mut self, len: usize) -> TaggedValue {
        if self.needs_collection(len) {
            self.collect();
        }

        let ptr = self.alloc_raw(len, HeapObjectType::ByteArray, 0);

        // Zero initialize
        unsafe {
            ptr::write_bytes(ptr, 0, len);
        }

        ptr_to_tagged(ptr)
    }

    /// Low-level allocation with header setup
    fn alloc_raw(&self, size: usize, obj_type: HeapObjectType, ptr_count: u8) -> *mut u8 {
        // Align size to 8 bytes
        let aligned_size = (size + 7) & !7;
        let total_size = HEAP_HEADER_SIZE + aligned_size;

        loop {
            let current = self.alloc_ptr.load(Ordering::Relaxed);
            let new_ptr = current + total_size;

            if new_ptr > self.from_space_end {
                panic!("[GC] Out of memory! Collection should have been triggered.");
            }

            if self.alloc_ptr.compare_exchange_weak(
                current, new_ptr, Ordering::SeqCst, Ordering::Relaxed
            ).is_ok() {
                let header_ptr = current as *mut HeapObjectHeader;
                let data_ptr = (current + HEAP_HEADER_SIZE) as *mut u8;

                unsafe {
                    (*header_ptr).forwarding = ptr::null_mut();
                    (*header_ptr).size = aligned_size as u32;
                    (*header_ptr).magic = HEAP_MAGIC;
                    (*header_ptr).obj_type = obj_type;
                    (*header_ptr).ptr_count = ptr_count;
                    (*header_ptr).flags = 0;
                    (*header_ptr)._pad = [0; 5];

                    // Zero the data
                    ptr::write_bytes(data_ptr, 0, aligned_size);
                }

                self.alloc_count.fetch_add(1, Ordering::Relaxed);
                self.bytes_allocated.fetch_add(total_size, Ordering::Relaxed);

                return data_ptr;
            }
        }
    }

    /// Copy a heap object to to-space, returning the new tagged value
    /// If already copied, returns the forwarding pointer
    unsafe fn copy_object(&self, v: TaggedValue, to_alloc: &mut *mut u8) -> TaggedValue {
        // Not a heap pointer or null? Return as-is
        if !is_heap_ptr(v) || v == 0 {
            return v;
        }

        // Already in to-space? Return as-is
        if self.is_to_space_ptr(v) {
            return v;
        }

        // Not in from-space? Error!
        if !self.is_from_space_ptr(v) {
            panic!("[GC] Pointer {:#x} is not in from-space!", v);
        }

        let header = get_header_mut(v);

        // Check for valid magic
        if header.magic != HEAP_MAGIC {
            panic!("[GC] Invalid object at {:#x} (bad magic: {:#x})",
                v, header.magic);
        }

        // Already forwarded?
        if !header.forwarding.is_null() {
            return header.forwarding as TaggedValue;
        }

        // Copy to to-space
        let size = header.size as usize;
        let total_size = HEAP_HEADER_SIZE + size;

        let new_header_ptr = *to_alloc as *mut HeapObjectHeader;
        let new_data_ptr = (*to_alloc).add(HEAP_HEADER_SIZE);

        // Copy entire object (header + data)
        let old_header_ptr = (v as *mut u8).sub(HEAP_HEADER_SIZE);
        ptr::copy_nonoverlapping(old_header_ptr, *to_alloc, total_size);

        // Clear forwarding in new copy
        (*new_header_ptr).forwarding = ptr::null_mut();

        // Set forwarding pointer in old location (points to data, not header)
        header.forwarding = new_data_ptr;

        // Advance to-space allocation pointer
        *to_alloc = to_alloc.add(total_size);

        self.move_count.fetch_add(1, Ordering::Relaxed);

        let new_tagged = ptr_to_tagged(new_data_ptr);

        if self.verbose {
            println!("[GC]   Copied {:?} {:#x} -> {:#x}",
                header.obj_type, v, new_tagged);
        }

        new_tagged
    }

    /// Scan an object for interior pointers and copy them
    unsafe fn scan_object(&self, v: TaggedValue, to_alloc: &mut *mut u8) {
        if !is_heap_ptr(v) {
            return;
        }

        let header = get_header(v);
        let ptr_count = header.ptr_count as usize;

        if ptr_count == 0 {
            return;
        }

        // Each pointer field is a TaggedValue
        let fields = v as *mut TaggedValue;

        for i in 0..ptr_count {
            let field_val = *fields.add(i);
            if is_heap_ptr(field_val) && self.is_from_space_ptr(field_val) {
                let new_val = self.copy_object(field_val, to_alloc);
                *fields.add(i) = new_val;
            }
        }
    }

    /// Perform garbage collection using Cheney's algorithm
    /// Roots are found by walking the shadow stack
    pub fn collect(&mut self) {
        self.force_gc.store(false, Ordering::Relaxed);

        let gc_num = self.gc_count.load(Ordering::Relaxed) + 1;

        if self.verbose {
            println!("\n[GC] ═══════════════════════════════════════════════════════════");
            println!("[GC] Starting GC #{}", gc_num);
        }

        // Reset to-space
        let mut to_alloc = self.to_space;
        let to_start = self.to_space;

        // Phase 1: Copy all roots from shadow stack
        if self.verbose {
            println!("[GC] Phase 1: Scanning shadow stack for roots...");
        }

        let mut roots_found = 0;
        let stack = shadow_stack();

        // We need to collect root pointers first, then update them
        // because we can't borrow the shadow stack mutably while iterating
        let mut root_slots: Vec<*mut TaggedValue> = Vec::new();

        stack.for_each_root(|slot| {
            root_slots.push(slot);
        });

        if self.verbose {
            println!("[GC]   Found {} heap pointer roots in shadow stack", root_slots.len());
        }

        // Now copy each root and update the slot
        for slot in root_slots.iter() {
            unsafe {
                let old_val = **slot;
                if self.is_from_space_ptr(old_val) {
                    let new_val = self.copy_object(old_val, &mut to_alloc);
                    **slot = new_val;
                    roots_found += 1;
                }
            }
        }

        self.roots_scanned.fetch_add(roots_found, Ordering::Relaxed);

        // Phase 2: Cheney's algorithm - scan copied objects for more pointers
        if self.verbose {
            println!("[GC] Phase 2: Scanning copied objects (Cheney's algorithm)...");
        }

        let mut scan_ptr = to_start;

        while (scan_ptr as usize) < (to_alloc as usize) {
            unsafe {
                // Read header at scan position
                let header = &*(scan_ptr as *const HeapObjectHeader);
                let data_ptr = scan_ptr.add(HEAP_HEADER_SIZE);
                let obj_size = header.size as usize;
                let total_size = HEAP_HEADER_SIZE + obj_size;

                let tagged = ptr_to_tagged(data_ptr);
                self.scan_object(tagged, &mut to_alloc);

                scan_ptr = scan_ptr.add(total_size);
            }
        }

        // Phase 3: Swap spaces
        if self.verbose {
            println!("[GC] Phase 3: Swapping spaces...");
        }

        std::mem::swap(&mut self.from_space, &mut self.to_space);
        self.alloc_ptr.store(to_alloc as usize, Ordering::Relaxed);
        self.from_space_end = self.from_space as usize + SEMI_SPACE_SIZE;

        // Poison old from-space (now to-space) for debugging
        unsafe {
            ptr::write_bytes(self.to_space, 0xDD, SEMI_SPACE_SIZE);
        }

        self.gc_count.fetch_add(1, Ordering::Relaxed);

        let used = to_alloc as usize - to_start as usize;
        let moved = self.move_count.load(Ordering::Relaxed);

        if self.verbose {
            println!("[GC] ═══════════════════════════════════════════════════════════");
            println!("[GC] GC #{} complete: {} bytes live, {} objects total moved", gc_num, used, moved);
            println!("[GC] ═══════════════════════════════════════════════════════════\n");
        }
    }

    /// Force a GC on the next allocation
    pub fn force_gc_on_next_alloc(&self) {
        self.force_gc.store(true, Ordering::Relaxed);
    }

    /// Print statistics
    pub fn print_stats(&self) {
        println!("[GC Stats]");
        println!("  Allocations: {}", self.alloc_count.load(Ordering::Relaxed));
        println!("  Bytes allocated: {}", self.bytes_allocated.load(Ordering::Relaxed));
        println!("  GC cycles: {}", self.gc_count.load(Ordering::Relaxed));
        println!("  Objects moved: {}", self.move_count.load(Ordering::Relaxed));
        println!("  Roots scanned: {}", self.roots_scanned.load(Ordering::Relaxed));

        let stack = shadow_stack();
        println!("  Shadow stack frames pushed: {}", stack.frames_pushed);
        println!("  Shadow stack max depth: {}", stack.max_depth);
    }
}

impl Drop for TaggedGcState {
    fn drop(&mut self) {
        unsafe {
            let layout = Layout::from_size_align(SEMI_SPACE_SIZE, 4096).unwrap();
            dealloc(self.space_a, layout);
            dealloc(self.space_b, layout);
        }
    }
}

// ============================================================================
// Global GC state
// ============================================================================

pub static mut TAGGED_GC: Option<TaggedGcState> = None;

/// Initialize the tagged GC
pub fn init() {
    unsafe {
        TAGGED_GC = Some(TaggedGcState::new());
    }
}

/// Get the GC (initializing if needed)
pub fn gc() -> &'static mut TaggedGcState {
    unsafe {
        if TAGGED_GC.is_none() {
            init();
        }
        TAGGED_GC.as_mut().unwrap()
    }
}

/// Reset the GC (for testing)
pub fn reset() {
    unsafe {
        TAGGED_GC = None;
        crate::shadow_stack::SHADOW_STACK = crate::shadow_stack::ShadowStack::new();
    }
    init();
}

// ============================================================================
// C-compatible API for use from generated code
// ============================================================================

/// Allocate a cons cell (called from generated code)
#[no_mangle]
pub extern "C" fn rt_cons(car: TaggedValue, cdr: TaggedValue) -> TaggedValue {
    gc().alloc_cons(car, cdr)
}

/// Allocate a vector (called from generated code)
#[no_mangle]
pub extern "C" fn rt_vector(len: u64) -> TaggedValue {
    gc().alloc_vector(len as usize)
}

/// Force a GC (called from generated code or for testing)
#[no_mangle]
pub extern "C" fn rt_gc() {
    gc().collect();
}

/// Get car of a cons cell
#[no_mangle]
pub extern "C" fn rt_car(v: TaggedValue) -> TaggedValue {
    car(v)
}

/// Get cdr of a cons cell
#[no_mangle]
pub extern "C" fn rt_cdr(v: TaggedValue) -> TaggedValue {
    cdr(v)
}

/// Print GC stats
#[no_mangle]
pub extern "C" fn rt_gc_stats() {
    gc().print_stats();
}

// ============================================================================
// Rust-friendly allocation API
// ============================================================================

/// Allocate a cons cell (Rust API)
pub fn cons(car: TaggedValue, cdr: TaggedValue) -> TaggedValue {
    gc().alloc_cons(car, cdr)
}

/// Allocate a vector (Rust API)
pub fn vector(len: usize) -> TaggedValue {
    gc().alloc_vector(len)
}

/// Allocate bytes (Rust API)
pub fn bytes(len: usize) -> TaggedValue {
    gc().alloc_bytes(len)
}

/// Trigger GC (Rust API)
pub fn collect() {
    gc().collect();
}

/// Force GC on next allocation
pub fn force_gc_next() {
    gc().force_gc_on_next_alloc();
}

/// Print stats (Rust API)
pub fn stats() {
    gc().print_stats();
}

/// Set verbose mode
pub fn set_verbose(v: bool) {
    gc().verbose = v;
}
