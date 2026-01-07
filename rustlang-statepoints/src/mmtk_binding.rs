//! MMTk Binding for LLVM Statepoint-based GC
//!
//! This module integrates MMTk with our LLVM statepoint infrastructure.
//! Key design:
//! - Keep LLVM statepoints for precise root discovery
//! - Use MMTk for actual memory management and GC algorithms
//! - Custom slot type handles tagged pointers

use std::sync::{Mutex, OnceLock, Condvar};
use std::thread::JoinHandle;
use std::time::Instant;
use std::sync::atomic::{AtomicUsize, Ordering};

use mmtk::util::copy::{CopySemantics, GCWorkerCopyContext};
use mmtk::util::{Address, ObjectReference};
use mmtk::util::heap::vm_layout::vm_layout;
use mmtk::util::opaque_pointer::*;
use mmtk::vm::*;
use mmtk::vm::slot::Slot;
use mmtk::{Mutator, MMTK, MMTKBuilder};
use mmtk::scheduler::GCWorker;
use mmtk::AllocationSemantics;

use crate::tagged_value::*;
use crate::stackmap::{StackMap, StackMapFunction, Location, LocationType};

fn gc_log_enabled() -> bool {
    static GC_LOG: OnceLock<bool> = OnceLock::new();
    *GC_LOG.get_or_init(|| std::env::var("GC_TRACE").is_ok())
}

fn root_trace_enabled() -> bool {
    static ROOT_LOG: OnceLock<bool> = OnceLock::new();
    *ROOT_LOG.get_or_init(|| std::env::var("STATEPOINT_ROOT_TRACE").is_ok())
}

fn gc_timing_enabled() -> bool {
    static GC_TIMING: OnceLock<bool> = OnceLock::new();
    *GC_TIMING.get_or_init(|| std::env::var("GC_TIMING").is_ok())
}

static GC_COUNT: AtomicUsize = AtomicUsize::new(0);
static ALLOC_COUNT: AtomicUsize = AtomicUsize::new(0);

macro_rules! gc_log {
    ($($t:tt)*) => {
        if gc_log_enabled() {
            eprintln!($($t)*);
        }
    };
}

/// Thread-local data for GC worker threads
struct WorkerThreadData {
    _marker: (),
}

// ============================================================================
// Global MMTk State
// ============================================================================

/// Global MMTk instance
static MMTK_INSTANCE: OnceLock<Box<MMTK<StatepointVM>>> = OnceLock::new();

/// GC worker thread handles for proper shutdown
static GC_WORKER_THREADS: Mutex<Vec<JoinHandle<()>>> = Mutex::new(Vec::new());

/// Wrapper for mutator pointer to make it Send+Sync
/// Safety: We only have one mutator thread, and we synchronize access via Mutex
struct MutatorPtr(*mut Mutator<StatepointVM>);
unsafe impl Send for MutatorPtr {}
unsafe impl Sync for MutatorPtr {}

/// Global mutator pointer for ActivePlan::mutators()
/// This must be set when bind_mutator is called
static MUTATOR_PTR: Mutex<Option<MutatorPtr>> = Mutex::new(None);

/// Synchronization for GC - mutator waits on this until GC completes
static GC_SYNC: (Mutex<bool>, Condvar) = (Mutex::new(false), Condvar::new());

/// Get the global MMTk instance
pub fn mmtk() -> &'static MMTK<StatepointVM> {
    MMTK_INSTANCE.get().expect("MMTk not initialized")
}

// Thread-local mutator - we store a raw pointer to avoid the borrowing issues
// Safety: The mutator is only accessed from the same thread, and it lives
// for the thread's lifetime
thread_local! {
    static MUTATOR: std::cell::Cell<Option<*mut Mutator<StatepointVM>>> =
        const { std::cell::Cell::new(None) };
}

// Storage for the mutator box to keep it alive
thread_local! {
    static MUTATOR_BOX: std::cell::RefCell<Option<Box<Mutator<StatepointVM>>>> =
        const { std::cell::RefCell::new(None) };
}

/// Global state for statepoint integration
pub struct StatepointState {
    /// Parsed stackmaps
    pub stackmap: Option<StackMap>,
    /// Base address of JIT code (unused when using absolute addresses)
    pub code_base: usize,
    /// Map from absolute return address to record index
    pub safepoint_map: std::collections::HashMap<u64, usize>,
    /// Current stack frame info for root scanning
    pub current_frame: Option<FrameInfo>,
    /// Saved register context for the current safepoint (AArch64)
    pub current_regs: Option<usize>,
    pub stack_low: usize,
    pub stack_high: usize,
}

/// Frame state stored in atomics for fast access (no mutex needed)
use std::sync::atomic::AtomicPtr;

static FRAME_FP: AtomicPtr<u8> = AtomicPtr::new(std::ptr::null_mut());
static FRAME_SP: AtomicPtr<u8> = AtomicPtr::new(std::ptr::null_mut());
static FRAME_RA: AtomicUsize = AtomicUsize::new(0);
static FRAME_REGS: AtomicUsize = AtomicUsize::new(0);

/// Stack bounds stored atomically for fast access during GC slot updates
static STACK_LOW: AtomicUsize = AtomicUsize::new(0);
static STACK_HIGH: AtomicUsize = AtomicUsize::new(0);

#[inline(always)]
fn set_frame_atomics(fp: *const u8, sp: *const u8, ra: usize, regs: Option<usize>) {
    // Use Relaxed ordering - GC will do a full synchronization anyway
    FRAME_FP.store(fp as *mut u8, Ordering::Relaxed);
    FRAME_SP.store(sp as *mut u8, Ordering::Relaxed);
    FRAME_RA.store(ra, Ordering::Relaxed);
    FRAME_REGS.store(regs.unwrap_or(0), Ordering::Relaxed);
}

#[inline(always)]
fn get_frame_atomics() -> Option<FrameInfo> {
    let fp = FRAME_FP.load(Ordering::Acquire);
    let sp = FRAME_SP.load(Ordering::Acquire);
    let ra = FRAME_RA.load(Ordering::Acquire);
    if fp.is_null() && sp.is_null() && ra == 0 {
        None
    } else {
        Some(FrameInfo { fp, sp, return_addr: ra })
    }
}

#[inline(always)]
fn get_regs_atomics() -> Option<usize> {
    let regs = FRAME_REGS.load(Ordering::Acquire);
    if regs == 0 { None } else { Some(regs) }
}

#[inline(always)]
fn clear_frame_atomics() {
    FRAME_FP.store(std::ptr::null_mut(), Ordering::Release);
    FRAME_SP.store(std::ptr::null_mut(), Ordering::Release);
    FRAME_RA.store(0, Ordering::Release);
    FRAME_REGS.store(0, Ordering::Release);
}

#[derive(Clone)]
pub struct FrameInfo {
    pub fp: *const u8,
    pub sp: *const u8,
    pub return_addr: usize,
}

// Safety: FrameInfo contains raw pointers that are only valid in specific contexts
// We ensure these are only accessed from the same thread during GC
unsafe impl Send for FrameInfo {}
unsafe impl Sync for FrameInfo {}

static STATEPOINT_STATE: Mutex<Option<StatepointState>> = Mutex::new(None);
static mut GLOBAL_ROOT: TaggedValue = 0;

/// Get the statepoint state
pub fn statepoint_state() -> std::sync::MutexGuard<'static, Option<StatepointState>> {
    STATEPOINT_STATE.lock().unwrap()
}

// ============================================================================
// VMBinding - The main trait that ties everything together
// ============================================================================

#[derive(Default)]
pub struct StatepointVM;

impl VMBinding for StatepointVM {
    type VMObjectModel = StatepointObjectModel;
    type VMScanning = StatepointScanning;
    type VMCollection = StatepointCollection;
    type VMActivePlan = StatepointActivePlan;
    type VMReferenceGlue = StatepointReferenceGlue;
    type VMSlot = TaggedSlot;
    type VMMemorySlice = mmtk::vm::slot::UnimplementedMemorySlice<TaggedSlot>;

    const MAX_ALIGNMENT: usize = 8;
    const MIN_ALIGNMENT: usize = 8;
}

// ============================================================================
// ObjectModel - How objects are laid out in memory
// ============================================================================

pub struct StatepointObjectModel;

impl ObjectModel<StatepointVM> for StatepointObjectModel {
    // Global log bit - first global side metadata
    const GLOBAL_LOG_BIT_SPEC: VMGlobalLogBitSpec = VMGlobalLogBitSpec::side_first();

    // Forwarding pointer in the header (offset 0 = forwarding field in HeapObjectHeader)
    const LOCAL_FORWARDING_POINTER_SPEC: VMLocalForwardingPointerSpec =
        VMLocalForwardingPointerSpec::in_header(0);
    const LOCAL_FORWARDING_BITS_SPEC: VMLocalForwardingBitsSpec =
        VMLocalForwardingBitsSpec::in_header(0);

    // Local side metadata - mark bit is first local, LOS follows it
    const LOCAL_MARK_BIT_SPEC: VMLocalMarkBitSpec = VMLocalMarkBitSpec::side_first();
    const LOCAL_LOS_MARK_NURSERY_SPEC: VMLocalLOSMarkNurserySpec =
        VMLocalLOSMarkNurserySpec::side_after(VMLocalMarkBitSpec::side_first().as_spec());

    const OBJECT_REF_OFFSET_LOWER_BOUND: isize = -(HEAP_HEADER_SIZE as isize);

    fn copy(
        from: ObjectReference,
        semantics: CopySemantics,
        copy_context: &mut GCWorkerCopyContext<StatepointVM>,
    ) -> ObjectReference {
        let bytes = Self::get_current_size(from);

        // alloc_copy returns the start of the allocation (where header goes)
        let dst_start = copy_context.alloc_copy(from, bytes, 8, 0, semantics);

        // Copy the object data (including our header)
        let src_addr = from.to_raw_address() - HEAP_HEADER_SIZE;
        let dst_addr = dst_start;

        unsafe {
            std::ptr::copy_nonoverlapping(
                src_addr.to_ptr::<u8>(),
                dst_addr.to_mut_ptr::<u8>(),
                bytes,
            );
        }

        // Clear forwarding pointer in new location
        let new_header = unsafe { &mut *(dst_addr.to_mut_ptr::<HeapObjectHeader>()) };
        new_header.forwarding = std::ptr::null_mut();

        // Object reference points to data, which is after the header
        let result_addr = dst_start + HEAP_HEADER_SIZE;
        let result = ObjectReference::from_raw_address(result_addr)
            .expect("Invalid address from alloc_copy");

        // Debug output disabled for performance:
        // println!("  GC COPY {:#x} -> {:#x}", from.to_raw_address().as_usize(), result.to_raw_address().as_usize());

        copy_context.post_copy(result, bytes, semantics);
        result
    }

    fn copy_to(_from: ObjectReference, _to: ObjectReference, _region: Address) -> Address {
        unimplemented!("copy_to not used by our GC plans")
    }

    fn get_current_size(object: ObjectReference) -> usize {
        let header = unsafe {
            let header_addr = object.to_raw_address() - HEAP_HEADER_SIZE;
            &*(header_addr.to_ptr::<HeapObjectHeader>())
        };
        HEAP_HEADER_SIZE + header.size as usize
    }

    fn get_size_when_copied(object: ObjectReference) -> usize {
        Self::get_current_size(object)
    }

    fn get_align_when_copied(_object: ObjectReference) -> usize {
        8
    }

    fn get_align_offset_when_copied(_object: ObjectReference) -> usize {
        0
    }

    fn get_reference_when_copied_to(_from: ObjectReference, to: Address) -> ObjectReference {
        ObjectReference::from_raw_address(to + HEAP_HEADER_SIZE)
            .expect("Invalid address in get_reference_when_copied_to")
    }

    fn get_type_descriptor(_reference: ObjectReference) -> &'static [i8] {
        &[]
    }

    fn ref_to_object_start(object: ObjectReference) -> Address {
        object.to_raw_address() - HEAP_HEADER_SIZE
    }

    fn ref_to_header(object: ObjectReference) -> Address {
        object.to_raw_address() - HEAP_HEADER_SIZE
    }

    fn dump_object(_object: ObjectReference) {
        // Debug output if needed
    }
}

// ============================================================================
// TaggedSlot - Custom slot type for tagged pointers
// ============================================================================

/// A slot containing a tagged value that may point to a heap object.
/// Lower 3 bits are tag: 000 = heap pointer
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TaggedSlot {
    addr: Address,
}

impl TaggedSlot {
    pub fn new(addr: Address) -> Self {
        TaggedSlot { addr }
    }

    #[allow(dead_code)]
    pub fn from_ptr(ptr: *mut TaggedValue) -> Self {
        TaggedSlot {
            addr: Address::from_mut_ptr(ptr),
        }
    }
}

impl Slot for TaggedSlot {
    fn load(&self) -> Option<ObjectReference> {
        let tagged_val = unsafe { *(self.addr.to_ptr::<TaggedValue>()) };

        // Check if this is a heap pointer (tag 000 and non-null)
        if is_heap_ptr(tagged_val) {
            let ptr = tagged_val as usize;
            if ptr & 0x7 != 0 {
                return None;
            }
            let addr = unsafe { Address::from_usize(ptr) };
            if !is_valid_object_ref(addr) {
                return None;
            }
            let obj = unsafe { ObjectReference::from_raw_address_unchecked(addr) };
            Some(obj)
        } else {
            None
        }
    }

    fn store(&self, object: ObjectReference) {
        // Skip store if SKIP_STORE is set (for debugging)
        if std::env::var("SKIP_STORE").is_ok() {
            return;
        }

        let slot_addr = self.addr.as_usize();
        let old_val = unsafe { *(self.addr.to_ptr::<TaggedValue>()) };
        let ptr = object.to_raw_address().as_usize() as TaggedValue;

        // Get actual stack bounds from atomics (fast)
        let stack_low = STACK_LOW.load(Ordering::Acquire);
        let stack_high = STACK_HIGH.load(Ordering::Acquire);

        // Classify the slot address:
        // - Heap: Use MMTk's actual vm_layout bounds
        // - Stack: between stack_low and stack_high (determined at bind_mutator)
        // - Global: everything else (static variables, etc.)
        let layout = vm_layout();
        let is_heap = slot_addr >= layout.heap_start.as_usize() && slot_addr < layout.heap_end.as_usize();
        let is_stack = stack_low > 0 && slot_addr >= stack_low && slot_addr < stack_high;

        if std::env::var("SLOT_TRACE").is_ok() && old_val != ptr {
            let loc = if is_heap { "heap" } else if is_stack { "stack" } else { "global" };
            eprintln!("SLOT UPDATE {}: {:#x} old={:#x} new={:#x}", loc, slot_addr, old_val, ptr);
        }

        // Control which slots to update for debugging
        if std::env::var("HEAP_ONLY").is_ok() {
            if !is_heap {
                return;
            }
        }

        if std::env::var("NO_STACK").is_ok() && is_stack {
            return;
        }

        // ptr should already have tag 000 since heap pointers are 8-byte aligned
        unsafe {
            *(self.addr.to_mut_ptr::<TaggedValue>()) = ptr;
        }
    }
}

// ============================================================================
// Scanning - Root discovery using LLVM stackmaps
// ============================================================================

pub struct StatepointScanning;

#[derive(Clone, Copy)]
struct FrameLayout {
    stack_size: usize,
    fp_offset: usize,
    lr_offset: usize,
}

fn frame_layout_from_stack_size(stack_size: usize) -> Option<FrameLayout> {
    // AArch64 keeps FP/LR in the top 16 bytes of the frame (sp + stack_size - 16/8).
    // We assume 16-byte alignment and a canonical frame-pointer prologue.
    #[cfg(not(target_arch = "aarch64"))]
    {
        let _ = stack_size;
        return None;
    }
    if stack_size < 16 || stack_size % 16 != 0 {
        return None;
    }
    Some(FrameLayout {
        stack_size,
        fp_offset: stack_size - 16,
        lr_offset: stack_size - 8,
    })
}

fn frame_layout_for_function(func: &StackMapFunction) -> Option<FrameLayout> {
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

    // Pattern A (main): stp x29, x30, [sp, #-16]!; mov x29, sp; sub sp, sp, #imm
    // Stack grows by (imm + 16). FP/LR live at SP + imm / SP + imm + 8.
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

fn function_for_pc<'a>(stackmap: &'a StackMap, pc: u64) -> Option<&'a StackMapFunction> {
    stackmap
        .functions
        .iter()
        .filter(|func| pc >= func.address)
        .max_by_key(|func| func.address)
}

fn lookup_safepoint_record(
    safepoint_map: &std::collections::HashMap<u64, usize>,
    ra: u64,
) -> Option<usize> {
    const DELTAS: [u64; 4] = [0, 4, 8, 12];
    for delta in DELTAS {
        if ra >= delta {
            if let Some(&idx) = safepoint_map.get(&(ra - delta)) {
                return Some(idx);
            }
        }
    }
    None
}

fn is_probable_heap_ptr(val: u64) -> bool {
    if val == 0 || val & 7 != 0 {
        return false;
    }
    let addr = unsafe { Address::from_usize(val as usize) };
    is_valid_object_ref(addr)
}

fn conservative_scan_frame(
    slots: &mut Vec<TaggedSlot>,
    frame_start: usize,
    frame_end: usize,
) -> usize {
    // Limit to avoid stack overflow in MMTk tracing
    let max_roots = std::env::var("STATEPOINT_MAX_ROOTS_PER_FRAME")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(10);

    let mut added = 0usize;
    let mut addr = frame_start;
    while addr + 8 <= frame_end && added < max_roots {
        let val = unsafe { std::ptr::read_unaligned(addr as *const u64) };
        if is_probable_heap_ptr(val) {
            let slot_addr = unsafe { Address::from_usize(addr) };
            slots.push(TaggedSlot::new(slot_addr));
            added += 1;
        }
        addr += 8;
    }
    added
}

fn is_valid_object_ref(addr: Address) -> bool {
    if addr.as_usize() < HEAP_HEADER_SIZE {
        return false;
    }
    let layout = vm_layout();
    // Log layout once for debugging
    static LAYOUT_LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if !LAYOUT_LOGGED.swap(true, Ordering::Relaxed) && std::env::var("HEAP_LAYOUT").is_ok() {
        eprintln!("HEAP LAYOUT: start={:#x} end={:#x}", layout.heap_start, layout.heap_end);
    }

    if addr < layout.heap_start || addr >= layout.heap_end {
        return false;
    }
    if !addr.is_mapped() {
        return false;
    }
    let header_addr = addr - HEAP_HEADER_SIZE;
    if !header_addr.is_mapped() {
        return false;
    }
    let header = unsafe { &*(header_addr.to_ptr::<HeapObjectHeader>()) };
    header.magic == HEAP_MAGIC && header.size > 0
}

impl Scanning<StatepointVM> for StatepointScanning {
    fn scan_roots_in_mutator_thread(
        _tls: VMWorkerThread,
        _mutator: &'static mut Mutator<StatepointVM>,
        mut factory: impl RootsWorkFactory<TaggedSlot>,
    ) {
        gc_log!("GC: scan_roots_in_mutator_thread called");

        // Minimal mode: only scan global root and top frame's saved registers
        // This avoids stack overflow in MMTk tracing while still maintaining correctness
        if std::env::var("STATEPOINT_MINIMAL_ROOTS").is_ok() {
            let mut slots = Vec::new();

            // Scan global root
            if unsafe { GLOBAL_ROOT } != 0 {
                let addr = unsafe { Address::from_mut_ptr(&raw mut GLOBAL_ROOT as *mut TaggedValue) };
                slots.push(TaggedSlot::new(addr));
                gc_log!("GC: minimal mode - added global root");
            }

            // Scan saved registers from the current frame (if available)
            if let Some(regs_ptr) = get_regs_atomics() {
                let regs = regs_ptr as *const usize;
                let mut reg_roots = 0;
                // Scan all general purpose registers (x0-x28 on AArch64)
                for i in 0..29 {
                    let reg_val = unsafe { *regs.add(i) };
                    if is_probable_heap_ptr(reg_val as u64) {
                        let reg_slot_addr = unsafe { Address::from_usize(regs_ptr + i * 8) };
                        slots.push(TaggedSlot::new(reg_slot_addr));
                        reg_roots += 1;
                    }
                }
                gc_log!("GC: minimal mode - added {} register roots", reg_roots);
            } else {
                gc_log!("GC: minimal mode - no regs available");
            }

            gc_log!("GC: minimal mode - total {} roots", slots.len());
            if !slots.is_empty() {
                factory.create_process_roots_work(slots);
            }
            return;
        }

        let state = statepoint_state();
        if state.is_none() {
            gc_log!("GC: no statepoint state!");
            return;
        }
        let state_ref = state.as_ref().unwrap();

        let mut slots = Vec::new();
        if unsafe { GLOBAL_ROOT } != 0 {
            if root_trace_enabled() {
                let val = unsafe { GLOBAL_ROOT };
                let addr = unsafe { Address::from_mut_ptr(&raw mut GLOBAL_ROOT as *mut TaggedValue) };
                let is_heap = is_heap_ptr(val);
                let is_valid = if is_heap {
                    is_valid_object_ref(unsafe { Address::from_usize(val as usize) })
                } else {
                    false
                };
                eprintln!(
                    "GC ROOT: slot={:#x} val={:#x} heap={} valid={}",
                    addr.as_usize(),
                    val,
                    is_heap,
                    is_valid
                );
            }
            let addr = unsafe { Address::from_mut_ptr(&raw mut GLOBAL_ROOT as *mut TaggedValue) };
            slots.push(TaggedSlot::new(addr));
        }

        // Get the initial frame saved when entering rt_cons (from atomics - fast!)
        let initial = match get_frame_atomics() {
            Some(f) => f,
            None => {
                gc_log!("GC: no current_frame!");
                if !slots.is_empty() {
                    factory.create_process_roots_work(slots);
                }
                return;
            }
        };

        let stackmap = match &state_ref.stackmap {
            Some(stackmap) => stackmap,
            None => {
                gc_log!("GC: no stackmap loaded");
                return;
            }
        };

        let target_ra = initial.return_addr;
        let stack_low = if state_ref.stack_low != 0 {
            state_ref.stack_low
        } else {
            initial.sp as usize
        };
        let stack_high = if state_ref.stack_high != 0 {
            state_ref.stack_high
        } else {
            stack_low + 0x800000
        };
        let mut current_ra = target_ra;
        let mut current_fp = 0usize;
        let mut current_sp = initial.sp as usize;
        let mut frame_depth = 1usize;
        let mut tail_scan_start: Option<usize> = None;

        loop {
            let func = match function_for_pc(stackmap, current_ra as u64) {
                Some(func) => func,
                None => {
                    gc_log!("GC: no function found for ra={:#x}", current_ra);
                    tail_scan_start = Some(current_sp);
                    break;
                }
            };
            let layout = match frame_layout_for_function(func) {
                Some(layout) => layout,
                None => {
                    gc_log!(
                        "GC: unsupported stack_size={} for ra={:#x}",
                        func.stack_size, current_ra
                    );
                    tail_scan_start = Some(current_sp);
                    break;
                }
            };
            if current_fp == 0 {
                current_fp = current_sp + layout.fp_offset;
            }
            if current_sp == 0 {
                if current_fp < layout.fp_offset {
                    break;
                }
                current_sp = current_fp - layout.fp_offset;
            }
            if current_sp < stack_low || current_sp > stack_high {
                tail_scan_start = Some(current_sp);
                break;
            }
            if current_fp < stack_low || current_fp > stack_high {
                tail_scan_start = Some(current_sp);
                break;
            }
            let mut precise_roots = 0usize;
            if let Some(record_idx) =
                lookup_safepoint_record(&state_ref.safepoint_map, current_ra as u64)
            {
                let record = &stackmap.records[record_idx];
                let gc_locs = stackmap.get_gc_locations(record);

                gc_log!(
                    "  record_idx={} has {} locs, {} GC locs, fp={:#x} sp={:#x}",
                    record_idx,
                    record.locations.len(),
                    gc_locs.len(),
                    current_fp,
                    current_sp
                );

                let regs_for_frame = if frame_depth == 1 {
                    get_regs_atomics()
                } else {
                    None
                };

                // For the first frame (where cons was called), also add the saved registers
                // x0 and x1 (car/cdr parameters) as roots since they're not tracked by statepoint
                if frame_depth == 1 {
                    if let Some(regs_ptr) = regs_for_frame {
                        let regs = regs_ptr as *const usize;
                        // Scan x0 and x1 (the car/cdr parameters to cons)
                        for reg_idx in 0..2 {
                            let reg_val = unsafe { *regs.add(reg_idx) };
                            if is_heap_ptr(reg_val as TaggedValue) {
                                let reg_slot_addr = unsafe { Address::from_usize(regs_ptr + reg_idx * 8) };
                                if std::env::var("ROOT_ADDR_TRACE").is_ok() {
                                    eprintln!("REG ROOT: x{} addr={:#x} val={:#x}", reg_idx, reg_slot_addr.as_usize(), reg_val);
                                }
                                slots.push(TaggedSlot::new(reg_slot_addr));
                                precise_roots += 1;
                            }
                        }
                    }
                }

                // Use precise scanning from statepoints by default
                let use_precise = std::env::var("SKIP_PRECISE").is_err();
                for (_i, (base_loc, derived_loc)) in gc_locs.iter().enumerate() {
                    if !use_precise { continue; }
                    let mut addr = resolve_location(
                        derived_loc,
                        current_fp as *const u8,
                        current_sp as *const u8,
                        regs_for_frame,
                        stack_low,
                        stack_high,
                    );
                    if addr.is_none() {
                        addr = resolve_location(
                            base_loc,
                            current_fp as *const u8,
                            current_sp as *const u8,
                            regs_for_frame,
                            stack_low,
                            stack_high,
                        );
                    }
                    if let Some(addr) = addr {
                        if addr.as_usize() & 7 != 0 {
                            continue;
                        }
                        let val = unsafe { std::ptr::read_unaligned(addr.to_ptr::<u64>()) };
                        gc_log!("    -> slot at {:?} = {:#x}", addr, val);

                        if is_heap_ptr(val as TaggedValue) {
                            if std::env::var("ROOT_ADDR_TRACE").is_ok() {
                                eprintln!("ROOT: addr={:#x} val={:#x}", addr.as_usize(), val);
                            }
                            gc_log!("      -> ADDED as root!");
                            slots.push(TaggedSlot::new(addr));
                            precise_roots += 1;
                        }
                    }
                }
            }
            // Only use conservative scanning if precise found nothing AND it's explicitly enabled
            if precise_roots == 0 && std::env::var("STATEPOINT_ENABLE_CONSERVATIVE").is_ok() {
                let frame_end = (current_sp + layout.stack_size).min(stack_high);
                conservative_scan_frame(&mut slots, current_sp, frame_end);
            }

            frame_depth += 1;
            // Limit frame depth to avoid stack overflow in MMTk tracing
            let max_frame_depth = std::env::var("STATEPOINT_MAX_FRAME_DEPTH")
                .ok()
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(100);
            if frame_depth > max_frame_depth {
                gc_log!("GC: Frame chain too deep, stopping at {}", max_frame_depth);
                break;
            }

            let lr_slot = match current_sp.checked_add(layout.lr_offset) {
                Some(val) => val,
                None => break,
            };
            if lr_slot + 8 > stack_high {
                gc_log!(
                    "GC: Frame chain ended at depth {} (LR slot out of bounds)",
                    frame_depth
                );
                tail_scan_start = Some(current_sp);
                break;
            }

            let caller_ra = unsafe { std::ptr::read_unaligned(lr_slot as *const usize) } as u64;
            if function_for_pc(stackmap, caller_ra).is_none() {
                gc_log!(
                    "GC: Frame chain ended at depth {} (caller ra not in JIT functions)",
                    frame_depth
                );
                tail_scan_start = Some(current_sp);
                break;
            }

            let caller_sp = match current_sp.checked_add(layout.stack_size) {
                Some(val) => val,
                None => break,
            };
            if caller_sp < stack_low || caller_sp > stack_high {
                gc_log!(
                    "GC: Frame chain ended at depth {} (caller sp out of bounds)",
                    frame_depth
                );
                tail_scan_start = Some(current_sp);
                break;
            }

            current_sp = caller_sp;
            current_fp = 0;
            current_ra = caller_ra as usize;

            // Safety limit
            // (handled above)
        }

        // Conservative tail scanning only if explicitly enabled
        if std::env::var("STATEPOINT_ENABLE_CONSERVATIVE").is_ok() {
            const MAX_CONSERVATIVE_SCAN: usize = 16 * 1024;
            if frame_depth <= 1 {
                let scan_end = (initial.sp as usize + MAX_CONSERVATIVE_SCAN).min(stack_high);
                conservative_scan_frame(&mut slots, initial.sp as usize, scan_end);
            } else if let Some(start) = tail_scan_start {
                let scan_start = start.max(stack_low);
                let scan_end = (scan_start + MAX_CONSERVATIVE_SCAN).min(stack_high);
                if scan_start < scan_end {
                    conservative_scan_frame(&mut slots, scan_start, scan_end);
                }
            }
        }
        gc_log!("GC: total roots found: {}", slots.len());
        if !slots.is_empty() {
            factory.create_process_roots_work(slots);
        }
        drop(state);
    }

    fn scan_vm_specific_roots(
        _tls: VMWorkerThread,
        mut _factory: impl RootsWorkFactory<TaggedSlot>,
    ) {
        if unsafe { GLOBAL_ROOT } != 0 {
            let addr = unsafe { Address::from_mut_ptr(&raw mut GLOBAL_ROOT as *mut TaggedValue) };
            gc_log!("GC: scanning global root slot at {:#x}", addr.as_usize());
            if root_trace_enabled() {
                let val = unsafe { GLOBAL_ROOT };
                let is_heap = is_heap_ptr(val);
                let is_valid = if is_heap {
                    is_valid_object_ref(unsafe { Address::from_usize(val as usize) })
                } else {
                    false
                };
                eprintln!(
                    "GC ROOT (vm): slot={:#x} val={:#x} heap={} valid={}",
                    addr.as_usize(),
                    val,
                    is_heap,
                    is_valid
                );
            }
            _factory.create_process_roots_work(vec![TaggedSlot::new(addr)]);
        }
    }

    fn scan_object<SV: SlotVisitor<TaggedSlot>>(
        _tls: VMWorkerThread,
        object: ObjectReference,
        slot_visitor: &mut SV,
    ) {
        let header = unsafe {
            let header_addr = object.to_raw_address() - HEAP_HEADER_SIZE;
            &*(header_addr.to_ptr::<HeapObjectHeader>())
        };

        // Only Cons cells are currently supported
        let HeapObjectType::Cons = header.obj_type;
        let obj_addr = object.to_raw_address();
        let car_slot = TaggedSlot::new(obj_addr);
        let cdr_slot = TaggedSlot::new(obj_addr + 8usize);
        slot_visitor.visit_slot(car_slot);
        slot_visitor.visit_slot(cdr_slot);
    }

    fn notify_initial_thread_scan_complete(_partial_scan: bool, _tls: VMWorkerThread) {
        // Nothing needed
    }

    fn supports_return_barrier() -> bool {
        false
    }

    fn prepare_for_roots_re_scanning() {
        // Nothing needed
    }
}

/// Resolve a stackmap location to a memory address
fn resolve_location(
    loc: &Location,
    fp: *const u8,
    sp: *const u8,
    regs: Option<usize>,
    stack_low: usize,
    stack_high: usize,
) -> Option<Address> {
    if loc.ty == LocationType::Register {
        let regs = regs? as *mut usize;
        if loc.offset != 0 {
            return None;
        }
        #[cfg(target_arch = "aarch64")]
        {
            let idx = loc.reg as usize;
            if idx > 31 {
                return None;
            }
            let slot = unsafe { regs.add(idx) };
            return Some(Address::from_ptr(slot as *const u8));
        }
        #[cfg(not(target_arch = "aarch64"))]
        {
            return None;
        }
    }

    #[cfg(target_arch = "aarch64")]
    let (base, base_from_regs) = match loc.reg {
        29 => {
            if let Some(regs) = regs {
                let regs = regs as *const usize;
                (unsafe { *(regs.add(29)) as *const u8 }, true)
            } else {
                (fp, false)
            }
        }
        31 => {
            if let Some(regs) = regs {
                let regs = regs as *const usize;
                (unsafe { *(regs.add(31)) as *const u8 }, true)
            } else {
                (sp, false)
            }
        }
        _ => {
            if let Some(regs) = regs {
                let regs = regs as *const usize;
                let idx = loc.reg as usize;
                if idx <= 30 {
                    (unsafe { *(regs.add(idx)) as *const u8 }, true)
                } else {
                    (sp, false)
                }
            } else {
                (sp, false)
            }
        }
    };
    #[cfg(target_arch = "x86_64")]
    let (base, base_from_regs) = match loc.reg {
        6 => (fp, false), // RBP
        7 => (sp, false), // RSP
        _ => {
            if let Some(regs) = regs {
                let regs = regs as *const usize;
                let idx = loc.reg as usize;
                (unsafe { *(regs.add(idx)) as *const u8 }, true)
            } else {
                (sp, false)
            }
        }
    };
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    let (base, base_from_regs) = (sp, false);

    if base_from_regs {
        let base_usize = base as usize;
        if base_usize < stack_low || base_usize > stack_high {
            return None;
        }
    }

    let base_usize = base as usize;
    let addr_usize = if loc.offset >= 0 {
        base_usize.checked_add(loc.offset as usize)?
    } else {
        base_usize.checked_sub((-loc.offset) as usize)?
    };
    let addr = addr_usize as *const u8;
    if addr_usize < stack_low || addr_usize + 8 > stack_high {
        return None;
    }
    match loc.ty {
        LocationType::Direct | LocationType::Indirect => {
            // For both Direct and Indirect:
            // - Direct: the GC pointer is stored at `addr`
            // - Indirect: `addr` contains a pointer, but for our purposes,
            //   the SLOT we need to track is still `addr` because that's
            //   where the stack variable lives that we need to update
            //
            // The key insight: we want the SLOT address (where to update),
            // not the value at that address. TaggedSlot::load will read
            // the value, and TaggedSlot::store will update it.
            Some(Address::from_ptr(addr))
        }
        _ => None,
    }
}

// ============================================================================
// Collection - GC triggering and coordination
// ============================================================================

pub struct StatepointCollection;

impl Collection<StatepointVM> for StatepointCollection {
    fn stop_all_mutators<F>(_tls: VMWorkerThread, mut mutator_visitor: F)
    where
        F: FnMut(&'static mut Mutator<StatepointVM>),
    {
        // Visit our single mutator so MMTk can scan its roots
        if let Ok(guard) = MUTATOR_PTR.lock() {
            if let Some(ref wrapper) = *guard {
                let mutator: &'static mut Mutator<StatepointVM> = unsafe { &mut *wrapper.0 };
                mutator_visitor(mutator);
            }
        }
    }

    fn resume_mutators(_tls: VMWorkerThread) {
        // Clear frame info now that GC is complete (using atomics - fast!)
        clear_frame_atomics();
        // Signal that GC is complete
        let (lock, cvar) = &GC_SYNC;
        let mut gc_done = lock.lock().unwrap();
        *gc_done = true;
        cvar.notify_all();
    }

    fn block_for_gc(_tls: VMMutatorThread) {
        // Block until GC is complete
        let (lock, cvar) = &GC_SYNC;
        let mut gc_done = lock.lock().unwrap();
        let timeout = std::time::Duration::from_secs(5);
        while !*gc_done {
            let result = cvar.wait_timeout(gc_done, timeout).unwrap();
            gc_done = result.0;
            if result.1.timed_out() {
                gc_log!("WARNING: GC block_for_gc timed out");
                return;
            }
        }
        *gc_done = false;
    }

    fn spawn_gc_thread(_tls: VMThread, ctx: GCThreadContext<StatepointVM>) {
        // Spawn a GC worker thread with a large stack for deep object graphs
        match ctx {
            GCThreadContext::Worker(worker) => {
                let handle = std::thread::Builder::new()
                    .name("mmtk-gc-worker".to_string())
                    .stack_size(64 * 1024 * 1024) // 64MB stack for deep object tracing
                    .spawn(move || {
                        // Create a valid TLS for this worker thread.
                        // We allocate a small struct on the heap and use its address as TLS.
                        // This must live for the duration of the worker.
                        let tls_data = Box::new(WorkerThreadData { _marker: () });
                        let tls_ptr = Box::into_raw(tls_data);
                        let worker_tls = VMWorkerThread(VMThread(OpaquePointer::from_address(
                            unsafe { Address::from_usize(tls_ptr as usize) }
                        )));

                        // Run the GC worker
                        worker.run(worker_tls, mmtk());

                        // Clean up TLS data (worker.run() only returns on shutdown)
                        unsafe { drop(Box::from_raw(tls_ptr)); }
                    })
                    .expect("Failed to spawn GC worker thread");

                // Store the handle for potential later joining
                if let Ok(mut handles) = GC_WORKER_THREADS.lock() {
                    handles.push(handle);
                }
            }
        }
    }

    fn out_of_memory(_tls: VMThread, _err_kind: mmtk::util::alloc::AllocationError) {
        panic!("Out of memory!");
    }

    fn schedule_finalization(_tls: VMWorkerThread) {
        // No finalizers supported
    }
}

// ============================================================================
// ActivePlan - Mutator management
// ============================================================================

pub struct StatepointActivePlan;

impl ActivePlan<StatepointVM> for StatepointActivePlan {
    fn is_mutator(_tls: VMThread) -> bool {
        true // All threads are mutators in single-threaded mode
    }

    fn mutator(_tls: VMMutatorThread) -> &'static mut Mutator<StatepointVM> {
        MUTATOR.with(|m| {
            let ptr = m.get().expect("Mutator not bound");
            // Safety: The mutator is only accessed from the same thread that created it,
            // and the underlying Box (in MUTATOR_BOX) keeps it alive
            unsafe { &mut *ptr }
        })
    }

    fn mutators<'a>() -> Box<dyn Iterator<Item = &'a mut Mutator<StatepointVM>> + 'a> {
        // Get the mutator pointer from our global storage
        let ptr_opt = MUTATOR_PTR.lock().ok().and_then(|guard| guard.as_ref().map(|p| p.0));
        match ptr_opt {
            Some(ptr) => {
                // Safety: The mutator lives as long as the program (stored in MUTATOR_BOX thread-local)
                // and we only have one mutator thread
                let mutator: &'a mut Mutator<StatepointVM> = unsafe { &mut *ptr };
                Box::new(std::iter::once(mutator))
            }
            None => Box::new(std::iter::empty()),
        }
    }

    fn number_of_mutators() -> usize {
        // Must be consistent with mutators() iterator length
        match MUTATOR_PTR.lock().ok().and_then(|guard| guard.as_ref().map(|_| ())) {
            Some(_) => 1,
            None => 0,
        }
    }

    fn vm_trace_object<Q: mmtk::ObjectQueue>(
        _queue: &mut Q,
        object: ObjectReference,
        _worker: &mut GCWorker<StatepointVM>,
    ) -> ObjectReference {
        // We don't need special tracing - just return the object
        object
    }
}

// ============================================================================
// ReferenceGlue - Weak references and finalization
// ============================================================================

pub struct StatepointReferenceGlue;

impl ReferenceGlue<StatepointVM> for StatepointReferenceGlue {
    type FinalizableType = ObjectReference;

    fn set_referent(_reference: ObjectReference, _referent: ObjectReference) {
        // No weak references
    }

    fn get_referent(_object: ObjectReference) -> Option<ObjectReference> {
        None
    }

    fn clear_referent(_object: ObjectReference) {
        // No weak references
    }

    fn enqueue_references(_references: &[ObjectReference], _tls: VMWorkerThread) {
        // No reference queues
    }
}

// ============================================================================
// Public API
// ============================================================================

/// Initialize MMTk with a specific plan
pub fn init_mmtk() {
    let mut builder = MMTKBuilder::new();

    // Set the plan (default: SemiSpace)
    let gc_plan = std::env::var("GC_PLAN").unwrap_or_else(|_| "SemiSpace".to_string());
    builder.set_option("plan", &gc_plan);

    // Set heap size - small heap to force GC
    let heap_mb = std::env::var("GC_HEAP_MB")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(16);
    builder.options.gc_trigger.set(
        mmtk::util::options::GCTriggerSelector::FixedHeapSize(heap_mb * 1024 * 1024),
    );

    let mmtk_instance = builder.build();
    MMTK_INSTANCE.set(Box::new(mmtk_instance)).ok();

    // Initialize statepoint state
    let mut state = STATEPOINT_STATE.lock().unwrap();
        *state = Some(StatepointState {
            stackmap: None,
            code_base: 0,
            safepoint_map: std::collections::HashMap::new(),
            current_frame: None,
            current_regs: None,
            stack_low: 0,
            stack_high: 0,
        });
}

/// Bind the current thread as a mutator
pub fn bind_mutator() {
    let tls = VMMutatorThread(VMThread::UNINITIALIZED);
    let mutator = mmtk::memory_manager::bind_mutator(mmtk(), tls);

    // Store the box to keep the mutator alive
    MUTATOR_BOX.with(|m| {
        let mut borrowed = m.borrow_mut();
        *borrowed = Some(mutator);
        // Get a raw pointer to the mutator
        let ptr = borrowed.as_mut().unwrap().as_mut() as *mut Mutator<StatepointVM>;
        MUTATOR.with(|mp| {
            mp.set(Some(ptr));
        });
        // Also store in global for ActivePlan::mutators()
        if let Ok(mut global_ptr) = MUTATOR_PTR.lock() {
            *global_ptr = Some(MutatorPtr(ptr));
        }
    });

    #[cfg(target_os = "macos")]
    {
        let pthread = unsafe { libc::pthread_self() };
        let stack_size = unsafe { libc::pthread_get_stacksize_np(pthread) };
        let stack_high = unsafe { libc::pthread_get_stackaddr_np(pthread) } as usize;
        let stack_low = stack_high.saturating_sub(stack_size);
        eprintln!("Mutator thread stack: size={}MB, low={:#x}, high={:#x}",
            stack_size / 1024 / 1024, stack_low, stack_high);
        // Store in atomics for fast access during GC
        STACK_LOW.store(stack_low, Ordering::Release);
        STACK_HIGH.store(stack_high, Ordering::Release);
        if let Ok(mut state) = STATEPOINT_STATE.lock() {
            if let Some(ref mut s) = *state {
                s.stack_low = stack_low;
                s.stack_high = stack_high;
            }
        }
    }
}

/// Enable collection
pub fn enable_collection() {
    let tls = VMThread::UNINITIALIZED;
    mmtk::memory_manager::initialize_collection(mmtk(), tls);
}

pub fn start_gc_stats() {
    let tls = VMMutatorThread(VMThread::UNINITIALIZED);
    mmtk().harness_begin(tls);
}

pub fn end_gc_stats() {
    use std::io::{self, BufRead, Write};
    use std::os::unix::io::FromRawFd;

    // Create a pipe to capture stdout
    let mut pipe_fds = [0i32; 2];
    unsafe {
        libc::pipe(pipe_fds.as_mut_ptr());
    }
    let read_fd = pipe_fds[0];
    let write_fd = pipe_fds[1];

    // Save original stdout
    let original_stdout = unsafe { libc::dup(1) };

    // Redirect stdout to our pipe
    unsafe {
        libc::dup2(write_fd, 1);
        libc::close(write_fd);
    }

    // Call harness_end (which prints to stdout)
    mmtk().harness_end();

    // Flush stdout
    io::stdout().flush().ok();

    // Restore original stdout
    unsafe {
        libc::dup2(original_stdout, 1);
        libc::close(original_stdout);
    }

    // Read captured output
    let mut captured = String::new();
    let read_file = unsafe { std::fs::File::from_raw_fd(read_fd) };
    let mut reader = io::BufReader::new(read_file);

    // Set read to non-blocking and read available data
    unsafe {
        let flags = libc::fcntl(read_fd, libc::F_GETFL);
        libc::fcntl(read_fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
    }

    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => captured.push_str(&line),
        }
    }

    // Parse and print formatted stats
    print_gc_summary_from_output(&captured);
}

fn print_gc_summary_from_output(output: &str) {
    let lines: Vec<&str> = output.lines().collect();

    // Find header and data lines
    let mut header_line = None;
    let mut data_line = None;
    let mut total_time = None;

    for (i, line) in lines.iter().enumerate() {
        if line.starts_with("GC\t") {
            header_line = Some(*line);
            if i + 1 < lines.len() {
                data_line = Some(lines[i + 1]);
            }
        }
        if line.starts_with("Total time:") {
            total_time = Some(*line);
        }
    }

    // Always print raw output for debugging
    if std::env::var("GC_RAW_STATS").is_ok() {
        print!("{}", output);
        return;
    }

    if header_line.is_none() || data_line.is_none() {
        // Fall back to printing raw output
        print!("{}", output);
        return;
    }

    let headers: Vec<&str> = header_line.unwrap().split('\t').collect();
    let values: Vec<&str> = data_line.unwrap().split('\t').collect();

    let stats: std::collections::HashMap<&str, &str> =
        headers.iter().cloned().zip(values.iter().cloned()).collect();

    println!("\n╔══════════════════════════════════════════════════════════════════╗");
    println!("║                        GC Statistics                             ║");
    println!("╠══════════════════════════════════════════════════════════════════╣");

    let gc_cycles: i64 = stats.get("GC").and_then(|s| s.parse().ok()).unwrap_or(0);
    println!("║  GC Cycles:         {:>10}                                  ║", gc_cycles);

    let mutator: f64 = stats.get("time.other").and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let stw: f64 = stats.get("time.stw").and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let total = mutator + stw;
    let stw_pct = if total > 0.0 { stw / total * 100.0 } else { 0.0 };

    println!("║  Mutator Time:      {:>10.2} ms                              ║", mutator);
    println!("║  STW Time:          {:>10.2} ms  ({:>5.1}%)                    ║", stw, stw_pct);

    // Warn if GC is thrashing (0 cycles but high STW time)
    if gc_cycles == 0 && stw > 1000.0 {
        println!("╠══════════════════════════════════════════════════════════════════╣");
        println!("║  ⚠ WARNING: GC started but never finished! (OOM)                ║");
        println!("║    Heap too small to complete collection - increase GC_HEAP_MB  ║");
    }
    println!("╠══════════════════════════════════════════════════════════════════╣");

    if let Some(count) = stats.get("total-work.count") {
        println!("║  Work Packets:      {:>10}                                  ║", count);
    }

    // Find PlanProcessEdges stats (main GC work)
    let mut edge_count = None;
    let mut edge_time = None;
    for (k, v) in &stats {
        if k.contains("PlanProcessEdges.count") {
            edge_count = Some(*v);
        }
        if k.contains("PlanProcessEdges.time.total") {
            edge_time = v.parse::<f64>().ok();
        }
    }

    if let Some(c) = edge_count {
        println!("║  Edge Packets:      {:>10}                                  ║", c);
    }
    if let Some(t) = edge_time {
        println!("║  Edge Work Time:    {:>10.2} ms                              ║", t);
    }

    println!("╠══════════════════════════════════════════════════════════════════╣");
    if let Some(tt) = total_time {
        println!("║  {}                                        ║", tt.trim());
    }
    println!("╚══════════════════════════════════════════════════════════════════╝");
}

/// Load stackmaps from compiled code
pub fn load_stackmaps(stackmap: StackMap, _code_base: usize) {
    let mut state = STATEPOINT_STATE.lock().unwrap();
    if let Some(ref mut s) = *state {
        let mut stackmap = stackmap;
        stackmap.functions.sort_by_key(|f| f.address);
        // Print function info for debugging (first function is code start)
        if let Some(first_func) = stackmap.functions.first() {
            gc_log!("JIT code starts at {:#x}, {} functions, {} safepoints",
                first_func.address, stackmap.functions.len(), stackmap.records.len());
        }

        // Use absolute_offset (which includes function address) as the key
        for (_idx, record) in stackmap.records.iter().enumerate() {
            s.safepoint_map.insert(record.absolute_offset, _idx);
        }

        if std::env::var("STATEPOINT_STACK_SIZES").is_ok() {
            let mut max_size = 0u64;
            for func in &stackmap.functions {
                if func.stack_size > max_size {
                    max_size = func.stack_size;
                }
            }
            eprintln!("JIT stackmap: {} functions, max stack_size={} bytes", stackmap.functions.len(), max_size);
        }

        s.code_base = _code_base;
        s.stackmap = Some(stackmap);
    }
}

/// Allocate an object
pub fn alloc(size: usize) -> Address {
    MUTATOR.with(|m| {
        let ptr = m.get().expect("Mutator not bound");
        let mutator = unsafe { &mut *ptr };

        let total_size = HEAP_HEADER_SIZE + size;
        let aligned_size = (total_size + 7) & !7;

        let addr = mmtk::memory_manager::alloc::<StatepointVM>(
            mutator,
            aligned_size,
            8,
            0,
            AllocationSemantics::Default,
        );

        // Return address after header
        addr + HEAP_HEADER_SIZE
    })
}

/// Allocate a cons cell
#[inline(always)]
pub fn alloc_cons(car: TaggedValue, cdr: TaggedValue) -> TaggedValue {
    MUTATOR.with(|m| {
        let mptr = m.get().expect("Mutator not bound");
        let mutator = unsafe { &mut *mptr };

        const SIZE: usize = std::mem::size_of::<ConsCell>();
        const TOTAL_SIZE: usize = HEAP_HEADER_SIZE + SIZE;
        const ALIGNED_SIZE: usize = (TOTAL_SIZE + 7) & !7;

        let header_addr = mmtk::memory_manager::alloc::<StatepointVM>(
            mutator,
            ALIGNED_SIZE,
            8,
            0,
            AllocationSemantics::Default,
        );

        // Set header
        let header = unsafe { &mut *(header_addr.to_mut_ptr::<HeapObjectHeader>()) };
        header.forwarding = std::ptr::null_mut();
        header.size = SIZE as u32;
        header.magic = HEAP_MAGIC;
        header.obj_type = HeapObjectType::Cons;
        header.ptr_count = 2;
        header.flags = 0;

        // Initialize cons cell
        let ptr = header_addr + HEAP_HEADER_SIZE;
        let cons = unsafe { &mut *(ptr.to_mut_ptr::<ConsCell>()) };
        cons.car = car;
        cons.cdr = cdr;

        // Post-alloc hook for MMTk
        let obj_ref = unsafe { ObjectReference::from_raw_address_unchecked(ptr) };
        mmtk::memory_manager::post_alloc::<StatepointVM>(
            mutator,
            obj_ref,
            ALIGNED_SIZE,
            AllocationSemantics::Default,
        );

        ptr.as_usize() as TaggedValue
    })
}

/// Trigger GC with current frame info
pub fn trigger_gc(fp: *const u8, _sp: *const u8, return_addr: usize) {
    let jit_sp = _sp;

    // Set current frame for root scanning (using atomics - fast!)
    set_frame_atomics(fp, jit_sp, return_addr, None);

    // Trigger GC with force=true to ensure it actually runs
    let tls = VMMutatorThread(VMThread::UNINITIALIZED);
    let (gc_id, start) = if gc_timing_enabled() {
        let id = GC_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
        let start = Instant::now();
        eprintln!("GC START {}", id);
        (Some(id), Some(start))
    } else {
        (None, None)
    };
    mmtk().handle_user_collection_request(tls, true, false);
    if let (Some(id), Some(start)) = (gc_id, start) {
        let elapsed = start.elapsed().as_millis();
        eprintln!("GC END {} ({} ms)", id, elapsed);
    }

    // NOTE: Don't clear frame info here - it will be cleared in resume_mutators
    // after scanning is complete
}

// ============================================================================
// Runtime functions callable from JIT code
// ============================================================================

/// Runtime: Allocate a cons cell (returns raw pointer for LLVM)
#[cfg(target_arch = "aarch64")]
#[no_mangle]
#[unsafe(naked)]
pub unsafe extern "C" fn rt_cons_raw_mmtk(_car: TaggedValue, _cdr: TaggedValue) -> *mut u8 {
    core::arch::naked_asm!(
        "sub sp, sp, #256",
        "stp x0, x1, [sp, #0]",
        "stp x2, x3, [sp, #16]",
        "stp x4, x5, [sp, #32]",
        "stp x6, x7, [sp, #48]",
        "stp x8, x9, [sp, #64]",
        "stp x10, x11, [sp, #80]",
        "stp x12, x13, [sp, #96]",
        "stp x14, x15, [sp, #112]",
        "stp x16, x17, [sp, #128]",
        "stp x18, x19, [sp, #144]",
        "stp x20, x21, [sp, #160]",
        "stp x22, x23, [sp, #176]",
        "stp x24, x25, [sp, #192]",
        "stp x26, x27, [sp, #208]",
        "stp x28, x29, [sp, #224]",
        "str x30, [sp, #240]",
        "add x15, sp, #256",
        "str x15, [sp, #248]",
        "mov x2, x29",
        "mov x3, x15",
        "mov x4, x30",
        "mov x5, sp",
        "bl {impl_fn}",
        "ldr x1, [sp, #8]",
        "ldr x2, [sp, #16]",
        "ldr x3, [sp, #24]",
        "ldr x4, [sp, #32]",
        "ldr x5, [sp, #40]",
        "ldr x6, [sp, #48]",
        "ldr x7, [sp, #56]",
        "ldr x8, [sp, #64]",
        "ldr x9, [sp, #72]",
        "ldr x10, [sp, #80]",
        "ldr x11, [sp, #88]",
        "ldr x12, [sp, #96]",
        "ldr x13, [sp, #104]",
        "ldr x14, [sp, #112]",
        "ldr x15, [sp, #120]",
        "ldr x16, [sp, #128]",
        "ldr x17, [sp, #136]",
        "ldr x18, [sp, #144]",
        "ldr x19, [sp, #152]",
        "ldr x20, [sp, #160]",
        "ldr x21, [sp, #168]",
        "ldr x22, [sp, #176]",
        "ldr x23, [sp, #184]",
        "ldr x24, [sp, #192]",
        "ldr x25, [sp, #200]",
        "ldr x26, [sp, #208]",
        "ldr x27, [sp, #216]",
        "ldr x28, [sp, #224]",
        "ldr x29, [sp, #232]",
        "ldr x30, [sp, #240]",
        "ldr x15, [sp, #248]",
        "mov sp, x15",
        "ret",
        impl_fn = sym rt_cons_raw_mmtk_impl,
    )
}

#[cfg(target_arch = "aarch64")]
#[inline(never)]
fn rt_cons_raw_mmtk_impl(
    _car: TaggedValue,  // Don't use these directly - they may be stale after GC
    _cdr: TaggedValue,
    fp: *const u8,
    sp: *const u8,
    ra: usize,
    regs: *mut usize,
) -> *mut u8 {
    // Set current frame for GC root scanning (using atomics - fast!)
    set_frame_atomics(fp, sp, ra, Some(regs as usize));

    // Allocate cons cell - GC may run during this, updating regs[0] and regs[1]
    // We read car/cdr from regs AFTER allocation since GC may have updated them
    MUTATOR.with(|m| {
        let mptr = m.get().expect("Mutator not bound");
        let mutator = unsafe { &mut *mptr };

        const SIZE: usize = std::mem::size_of::<ConsCell>();
        const TOTAL_SIZE: usize = HEAP_HEADER_SIZE + SIZE;
        const ALIGNED_SIZE: usize = (TOTAL_SIZE + 7) & !7;

        let header_addr = mmtk::memory_manager::alloc::<StatepointVM>(
            mutator,
            ALIGNED_SIZE,
            8,
            0,
            AllocationSemantics::Default,
        );

        // Re-read car/cdr from regs AFTER allocation (GC may have updated them)
        let car = unsafe { *regs } as TaggedValue;
        let cdr = unsafe { *regs.add(1) } as TaggedValue;

        // Set header
        let header = unsafe { &mut *(header_addr.to_mut_ptr::<HeapObjectHeader>()) };
        header.forwarding = std::ptr::null_mut();
        header.size = SIZE as u32;
        header.magic = HEAP_MAGIC;
        header.obj_type = HeapObjectType::Cons;
        header.ptr_count = 2;
        header.flags = 0;

        // Initialize cons cell with the potentially-updated values
        let ptr = header_addr + HEAP_HEADER_SIZE;
        let cons = unsafe { &mut *(ptr.to_mut_ptr::<ConsCell>()) };
        cons.car = car;
        cons.cdr = cdr;

        // Post-alloc hook for MMTk
        let obj_ref = unsafe { ObjectReference::from_raw_address_unchecked(ptr) };
        mmtk::memory_manager::post_alloc::<StatepointVM>(
            mutator,
            obj_ref,
            ALIGNED_SIZE,
            AllocationSemantics::Default,
        );

        ptr.as_usize() as *mut u8
    })
}

/// Allocate a cons cell, reading car/cdr from saved registers
/// This allows GC to update the register values if objects move
#[inline(always)]
fn alloc_cons_from_regs(regs: *mut usize) -> TaggedValue {
    MUTATOR.with(|m| {
        let mptr = m.get().expect("Mutator not bound");
        let mutator = unsafe { &mut *mptr };

        const SIZE: usize = std::mem::size_of::<ConsCell>();
        const TOTAL_SIZE: usize = HEAP_HEADER_SIZE + SIZE;
        const ALIGNED_SIZE: usize = (TOTAL_SIZE + 7) & !7;

        let header_addr = mmtk::memory_manager::alloc::<StatepointVM>(
            mutator,
            ALIGNED_SIZE,
            8,
            0,
            AllocationSemantics::Default,
        );

        // Set header
        let header = unsafe { &mut *(header_addr.to_mut_ptr::<HeapObjectHeader>()) };
        header.forwarding = std::ptr::null_mut();
        header.size = SIZE as u32;
        header.magic = HEAP_MAGIC;
        header.obj_type = HeapObjectType::Cons;
        header.ptr_count = 2;
        header.flags = 0;

        // Read car/cdr from saved regs AFTER allocation (GC may have updated them)
        let car = unsafe { *regs.add(0) } as TaggedValue;
        let cdr = unsafe { *regs.add(1) } as TaggedValue;

        // Initialize cons cell
        let ptr = header_addr + HEAP_HEADER_SIZE;
        let cons = unsafe { &mut *(ptr.to_mut_ptr::<ConsCell>()) };
        cons.car = car;
        cons.cdr = cdr;

        // Post-alloc hook for MMTk
        let obj_ref = unsafe { ObjectReference::from_raw_address_unchecked(ptr) };
        mmtk::memory_manager::post_alloc::<StatepointVM>(
            mutator,
            obj_ref,
            ALIGNED_SIZE,
            AllocationSemantics::Default,
        );

        ptr.as_usize() as TaggedValue
    })
}

#[cfg(not(target_arch = "aarch64"))]
#[no_mangle]
#[inline(never)]
pub extern "C" fn rt_cons_raw_mmtk(car: TaggedValue, cdr: TaggedValue) -> *mut u8 {
    #[cfg(target_arch = "x86_64")]
    let (fp, sp, ra) = unsafe {
        let fp: usize;
        let sp: usize;
        std::arch::asm!(
            "mov {fp}, rbp",
            "mov {sp}, rsp",
            fp = out(reg) fp,
            sp = out(reg) sp,
            options(nomem, nostack)
        );
        let ra = *((fp + 8) as *const usize);
        (fp as *const u8, sp as *const u8, ra)
    };
    #[cfg(not(target_arch = "x86_64"))]
    let (fp, sp, ra) = (std::ptr::null(), std::ptr::null(), 0usize);

    // Set current frame for GC root scanning (using atomics - fast!)
    set_frame_atomics(fp, sp, ra, None);

    alloc_cons(car, cdr) as *mut u8
}

/// Runtime: Trigger GC
#[cfg(target_arch = "aarch64")]
#[no_mangle]
#[unsafe(naked)]
pub unsafe extern "C" fn rt_gc_mmtk() {
    core::arch::naked_asm!(
        "sub sp, sp, #256",
        "stp x0, x1, [sp, #0]",
        "stp x2, x3, [sp, #16]",
        "stp x4, x5, [sp, #32]",
        "stp x6, x7, [sp, #48]",
        "stp x8, x9, [sp, #64]",
        "stp x10, x11, [sp, #80]",
        "stp x12, x13, [sp, #96]",
        "stp x14, x15, [sp, #112]",
        "stp x16, x17, [sp, #128]",
        "stp x18, x19, [sp, #144]",
        "stp x20, x21, [sp, #160]",
        "stp x22, x23, [sp, #176]",
        "stp x24, x25, [sp, #192]",
        "stp x26, x27, [sp, #208]",
        "stp x28, x29, [sp, #224]",
        "str x30, [sp, #240]",
        "add x15, sp, #256",
        "str x15, [sp, #248]",
        "mov x0, x29",
        "mov x1, x15",
        "mov x2, x30",
        "mov x3, sp",
        "bl {impl_fn}",
        "ldr x0, [sp, #0]",
        "ldr x1, [sp, #8]",
        "ldr x2, [sp, #16]",
        "ldr x3, [sp, #24]",
        "ldr x4, [sp, #32]",
        "ldr x5, [sp, #40]",
        "ldr x6, [sp, #48]",
        "ldr x7, [sp, #56]",
        "ldr x8, [sp, #64]",
        "ldr x9, [sp, #72]",
        "ldr x10, [sp, #80]",
        "ldr x11, [sp, #88]",
        "ldr x12, [sp, #96]",
        "ldr x13, [sp, #104]",
        "ldr x14, [sp, #112]",
        "ldr x15, [sp, #120]",
        "ldr x16, [sp, #128]",
        "ldr x17, [sp, #136]",
        "ldr x18, [sp, #144]",
        "ldr x19, [sp, #152]",
        "ldr x20, [sp, #160]",
        "ldr x21, [sp, #168]",
        "ldr x22, [sp, #176]",
        "ldr x23, [sp, #184]",
        "ldr x24, [sp, #192]",
        "ldr x25, [sp, #200]",
        "ldr x26, [sp, #208]",
        "ldr x27, [sp, #216]",
        "ldr x28, [sp, #224]",
        "ldr x29, [sp, #232]",
        "ldr x30, [sp, #240]",
        "ldr x15, [sp, #248]",
        "mov sp, x15",
        "ret",
        impl_fn = sym rt_gc_mmtk_impl,
    )
}

#[cfg(target_arch = "aarch64")]
#[inline(never)]
fn rt_gc_mmtk_impl(fp: *const u8, sp: *const u8, ra: usize, regs: *mut usize) {
    // Set current frame with regs (using atomics - fast!)
    set_frame_atomics(fp, sp, ra, Some(regs as usize));

    // Trigger GC
    let tls = VMMutatorThread(VMThread::UNINITIALIZED);
    let (gc_id, start) = if gc_timing_enabled() {
        let id = GC_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
        let start = Instant::now();
        eprintln!("GC START {}", id);
        (Some(id), Some(start))
    } else {
        (None, None)
    };
    mmtk().handle_user_collection_request(tls, true, false);
    if let (Some(id), Some(start)) = (gc_id, start) {
        let elapsed = start.elapsed().as_millis();
        eprintln!("GC END {} ({} ms)", id, elapsed);
    }
}

#[cfg(not(target_arch = "aarch64"))]
#[no_mangle]
#[inline(never)]
pub extern "C" fn rt_gc_mmtk() {
    #[cfg(target_arch = "x86_64")]
    let (fp, sp, ra) = unsafe {
        let fp: usize;
        let sp: usize;
        std::arch::asm!(
            "mov {fp}, rbp",
            "mov {sp}, rsp",
            fp = out(reg) fp,
            sp = out(reg) sp,
            options(nomem, nostack)
        );
        let ra = *((fp + 8) as *const usize);
        (fp as *const u8, sp as *const u8, ra)
    };
    #[cfg(not(target_arch = "x86_64"))]
    let (fp, sp, ra) = (std::ptr::null(), std::ptr::null(), 0usize);

    trigger_gc(fp, sp, ra);
}

/// Safepoint-based cons allocation for MMTk
/// Called from JIT code at safepoint with frame info from LLVM intrinsics
/// This replaces the naked assembly approach with proper statepoint usage
///
/// car_slot/cdr_slot are pointers to stack slots containing car/cdr values.
/// GC will update these slots if objects move, so we read from them AFTER allocation.
#[no_mangle]
pub extern "C" fn rt_cons_raw_mmtk_safepoint(
    car_slot: *mut TaggedValue,
    cdr_slot: *mut TaggedValue,
    fp: usize,
    sp: usize,
    ra: usize,
) -> *mut u8 {
    // Store frame info for GC to use during stack walking
    set_frame_atomics(fp as *const u8, sp as *const u8, ra, None);

    // Allocate cons cell - GC may run during this
    // GC will use the stackmap to find roots and update car_slot/cdr_slot if objects move
    MUTATOR.with(|m| {
        let mptr = m.get().expect("Mutator not bound");
        let mutator = unsafe { &mut *mptr };

        const SIZE: usize = std::mem::size_of::<ConsCell>();
        const TOTAL_SIZE: usize = HEAP_HEADER_SIZE + SIZE;
        const ALIGNED_SIZE: usize = (TOTAL_SIZE + 7) & !7;

        let header_addr = mmtk::memory_manager::alloc::<StatepointVM>(
            mutator,
            ALIGNED_SIZE,
            8,
            0,
            AllocationSemantics::Default,
        );

        // Read car/cdr from slots AFTER allocation (GC may have updated them)
        let car = unsafe { *car_slot };
        let cdr = unsafe { *cdr_slot };

        // Set header
        let header = unsafe { &mut *(header_addr.to_mut_ptr::<HeapObjectHeader>()) };
        header.forwarding = std::ptr::null_mut();
        header.size = SIZE as u32;
        header.magic = HEAP_MAGIC;
        header.obj_type = HeapObjectType::Cons;
        header.ptr_count = 2;
        header.flags = 0;

        // Initialize cons cell with potentially-updated values
        let ptr = header_addr + HEAP_HEADER_SIZE;
        let cons = unsafe { &mut *(ptr.to_mut_ptr::<ConsCell>()) };
        cons.car = car;
        cons.cdr = cdr;

        // Post-alloc hook for MMTk
        let obj_ref = unsafe { ObjectReference::from_raw_address_unchecked(ptr) };
        mmtk::memory_manager::post_alloc::<StatepointVM>(
            mutator,
            obj_ref,
            ALIGNED_SIZE,
            AllocationSemantics::Default,
        );

        ptr.as_usize() as *mut u8
    })
}

/// Runtime car - extract first element of cons cell
#[no_mangle]
#[inline(always)]
pub extern "C" fn rt_car(cell: TaggedValue) -> TaggedValue {
    let ptr = cell as usize as *const ConsCell;
    unsafe { (*ptr).car }
}

/// Runtime cdr - extract rest of cons cell
#[no_mangle]
#[inline(always)]
pub extern "C" fn rt_cdr(cell: TaggedValue) -> TaggedValue {
    let ptr = cell as usize as *const ConsCell;
    unsafe { (*ptr).cdr }
}

/// Runtime root management - keep a single global root alive
#[no_mangle]
pub extern "C" fn rt_set_global_root(val: TaggedValue) {
    unsafe {
        GLOBAL_ROOT = val;
    }
    gc_log!("GC: set global root to {:#x}", val);
}

#[no_mangle]
pub extern "C" fn rt_clear_global_root() {
    unsafe {
        GLOBAL_ROOT = 0;
    }
    gc_log!("GC: cleared global root");
}

/// Runtime print - print a tagged value
#[no_mangle]
pub extern "C" fn rt_print(val: TaggedValue) {
    use crate::tagged_value::{is_fixnum, fixnum_value, is_nil, is_true, is_false, is_pointer};

    if is_fixnum(val) {
        println!("{}", fixnum_value(val));
    } else if is_nil(val) {
        println!("nil");
    } else if is_true(val) {
        println!("true");
    } else if is_false(val) {
        println!("false");
    } else if is_pointer(val) {
        println!("<object @ {:#x}>", val);
    } else {
        println!("<unknown: {:#x}>", val);
    }
}

/// Runtime print-list - print a list in (a b c) format
#[no_mangle]
pub extern "C" fn rt_print_list(mut val: TaggedValue) {
    use crate::tagged_value::{is_fixnum, fixnum_value, is_nil, is_true, is_false, is_pointer};

    print!("(");
    let mut first = true;
    let mut count = 0;
    while !is_nil(val) && count < 20 {
        count += 1;
        if !first {
            print!(" ");
        }
        first = false;

        let car = rt_car(val);
        if is_fixnum(car) {
            print!("{}", fixnum_value(car));
        } else if is_nil(car) {
            print!("nil");
        } else if is_true(car) {
            print!("true");
        } else if is_false(car) {
            print!("false");
        } else if is_pointer(car) {
            // Nested list - recurse
            rt_print_list(car);
        } else {
            print!("?");
        }

        val = rt_cdr(val);
    }
    println!(")");
}

/// Runtime: print "stretch tree of depth X\t check: Y"
#[no_mangle]
pub extern "C" fn rt_print_stretch_check(depth: TaggedValue, check: TaggedValue) {
    use crate::tagged_value::fixnum_value;
    println!("stretch tree of depth {}\t check: {}", fixnum_value(depth), fixnum_value(check));
}

/// Runtime: print "N\t trees of depth X\t check: Y"
#[no_mangle]
pub extern "C" fn rt_print_trees_check(iters: TaggedValue, depth: TaggedValue, check: TaggedValue) {
    use crate::tagged_value::fixnum_value;
    println!("{}\t trees of depth {}\t check: {}", fixnum_value(iters), fixnum_value(depth), fixnum_value(check));
}

/// Runtime: print "long lived tree of depth X\t check: Y"
#[no_mangle]
pub extern "C" fn rt_print_long_lived_check(depth: TaggedValue, check: TaggedValue) {
    use crate::tagged_value::fixnum_value;
    println!("long lived tree of depth {}\t check: {}", fixnum_value(depth), fixnum_value(check));
}

/// Runtime: get command line argument (returns tagged fixnum, or 0 if not present)
#[no_mangle]
pub extern "C" fn rt_get_arg(idx: TaggedValue) -> TaggedValue {
    use crate::tagged_value::{fixnum_value, make_fixnum};
    let idx = fixnum_value(idx) as usize;
    let args: Vec<String> = std::env::args().collect();
    if idx < args.len() {
        if let Ok(n) = args[idx].parse::<i64>() {
            return make_fixnum(n);
        }
    }
    make_fixnum(0)
}

/// Runtime: max of two fixnums
#[no_mangle]
pub extern "C" fn rt_max(a: TaggedValue, b: TaggedValue) -> TaggedValue {
    use crate::tagged_value::{fixnum_value, make_fixnum};
    let a_val = fixnum_value(a);
    let b_val = fixnum_value(b);
    make_fixnum(std::cmp::max(a_val, b_val))
}

/// Runtime symbols struct
pub struct RuntimeSymbols {
    pub cons: usize,
    pub cons_safepoint: usize,  // New safepoint-based cons
    pub gc: usize,
    pub car: usize,
    pub cdr: usize,
    pub set_global_root: usize,
    pub clear_global_root: usize,
    pub print: usize,
    pub print_list: usize,
    pub print_stretch_check: usize,
    pub print_trees_check: usize,
    pub print_long_lived_check: usize,
    pub get_arg: usize,
    pub max: usize,
}

/// Get all runtime symbols
pub fn get_all_runtime_symbols() -> RuntimeSymbols {
    RuntimeSymbols {
        cons: rt_cons_raw_mmtk as *const () as usize,
        cons_safepoint: rt_cons_raw_mmtk_safepoint as *const () as usize,
        gc: rt_gc_mmtk as *const () as usize,
        car: rt_car as *const () as usize,
        cdr: rt_cdr as *const () as usize,
        set_global_root: rt_set_global_root as *const () as usize,
        clear_global_root: rt_clear_global_root as *const () as usize,
        print: rt_print as *const () as usize,
        print_list: rt_print_list as *const () as usize,
        print_stretch_check: rt_print_stretch_check as *const () as usize,
        print_trees_check: rt_print_trees_check as *const () as usize,
        print_long_lived_check: rt_print_long_lived_check as *const () as usize,
        get_arg: rt_get_arg as *const () as usize,
        max: rt_max as *const () as usize,
    }
}
