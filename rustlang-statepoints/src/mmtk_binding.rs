//! MMTk Binding for LLVM Statepoint-based GC
//!
//! This module integrates MMTk with our LLVM statepoint infrastructure.
//! Key design:
//! - Keep LLVM statepoints for precise root discovery
//! - Use MMTk for actual memory management and GC algorithms
//! - Custom slot type handles tagged pointers

use std::sync::{Mutex, OnceLock, Condvar};
use std::thread::JoinHandle;

use mmtk::util::alloc::AllocatorSelector;
use mmtk::util::copy::{CopySemantics, GCWorkerCopyContext};
use mmtk::util::{Address, ObjectReference};
use mmtk::util::opaque_pointer::*;
use mmtk::vm::*;
use mmtk::vm::slot::Slot;
use mmtk::{Mutator, MMTK, MMTKBuilder};
use mmtk::scheduler::GCWorker;
use mmtk::AllocationSemantics;

use crate::tagged_value::*;
use crate::stackmap::{StackMap, Location, LocationType};

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

/// Thread-local mutator - we store a raw pointer to avoid the borrowing issues
/// Safety: The mutator is only accessed from the same thread, and it lives
/// for the thread's lifetime
thread_local! {
    static MUTATOR: std::cell::Cell<Option<*mut Mutator<StatepointVM>>> =
        const { std::cell::Cell::new(None) };
}

/// Storage for the mutator box to keep it alive
thread_local! {
    static MUTATOR_BOX: std::cell::RefCell<Option<Box<Mutator<StatepointVM>>>> =
        const { std::cell::RefCell::new(None) };
}

/// Global state for statepoint integration
pub struct StatepointState {
    /// Parsed stackmaps
    pub stackmap: Option<StackMap>,
    /// Base address of JIT code
    pub code_base: usize,
    /// Map from instruction offset to record index
    pub safepoint_map: std::collections::HashMap<u32, usize>,
    /// Verbose mode
    pub verbose: bool,
    /// Current stack frame info for root scanning
    pub current_frame: Option<FrameInfo>,
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

        println!("  GC COPY {:#x} -> {:#x}", from.to_raw_address().as_usize(), result.to_raw_address().as_usize());

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
        ObjectReference::from_raw_address(to)
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
            // Verify it looks like a valid heap pointer
            if ptr > 0x1000 && ptr & 0x7 == 0 {
                let addr = unsafe { Address::from_usize(ptr) };
                Some(unsafe { ObjectReference::from_raw_address_unchecked(addr) })
            } else {
                None
            }
        } else {
            None
        }
    }

    fn store(&self, object: ObjectReference) {
        let old_val = unsafe { *(self.addr.to_ptr::<TaggedValue>()) };
        let ptr = object.to_raw_address().as_usize() as TaggedValue;
        println!("  ROOT UPDATE: slot at {:p} changed {:#x} -> {:#x}", self.addr.to_ptr::<u8>(), old_val, ptr);
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

impl Scanning<StatepointVM> for StatepointScanning {
    fn scan_roots_in_mutator_thread(
        _tls: VMWorkerThread,
        _mutator: &'static mut Mutator<StatepointVM>,
        mut factory: impl RootsWorkFactory<TaggedSlot>,
    ) {
        // Get the current frame info from statepoint state
        let state = statepoint_state();
        if let Some(ref state) = *state {
            if let Some(ref frame) = state.current_frame {
                // Find the safepoint record for this return address
                if state.code_base > 0 && frame.return_addr >= state.code_base {
                    let offset = (frame.return_addr - state.code_base) as u32;

                    // Look for safepoint
                    if let Some(&record_idx) = state.safepoint_map.get(&offset) {
                        if let Some(ref stackmap) = state.stackmap {
                            let record = &stackmap.records[record_idx];
                            let gc_locs = stackmap.get_gc_locations(record);

                            let mut slots = Vec::new();
                            for (_base_loc, derived_loc) in gc_locs.iter() {
                                if let Some(addr) =
                                    resolve_location(&derived_loc, frame.fp, frame.sp)
                                {
                                    slots.push(TaggedSlot::new(addr));
                                }
                            }

                            if !slots.is_empty() {
                                factory.create_process_roots_work(slots);
                            }
                        }
                    }
                }
            }
        }
        drop(state);
    }

    fn scan_vm_specific_roots(
        _tls: VMWorkerThread,
        _factory: impl RootsWorkFactory<TaggedSlot>,
    ) {
        // We could add global roots here if needed
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

        match header.obj_type {
            HeapObjectType::Cons => {
                // Cons cell has car and cdr fields
                let obj_addr = object.to_raw_address();
                let car_slot = TaggedSlot::new(obj_addr);
                let cdr_slot = TaggedSlot::new(obj_addr + 8usize);
                slot_visitor.visit_slot(car_slot);
                slot_visitor.visit_slot(cdr_slot);
            }
            HeapObjectType::Vector => {
                // Vector: first 8 bytes is length, then elements
                let obj_addr = object.to_raw_address();
                let len = unsafe { *(obj_addr.to_ptr::<u64>()) } as usize;
                for i in 0..len {
                    let slot = TaggedSlot::new(obj_addr + (8 + i * 8));
                    slot_visitor.visit_slot(slot);
                }
            }
            HeapObjectType::Closure => {
                // Closure: code ptr + captured vars
                // Skip first 8 bytes (code pointer)
                let obj_addr = object.to_raw_address();
                let num_captured = header.ptr_count as usize;
                for i in 0..num_captured {
                    let slot = TaggedSlot::new(obj_addr + (8 + i * 8));
                    slot_visitor.visit_slot(slot);
                }
            }
            _ => {
                // String, ByteArray, Symbol - no pointers to scan
            }
        }
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
fn resolve_location(loc: &Location, fp: *const u8, sp: *const u8) -> Option<Address> {
    match loc.ty {
        LocationType::Indirect | LocationType::Direct => {
            #[cfg(target_arch = "aarch64")]
            let base = match loc.reg {
                29 => fp, // x29 = FP
                31 => sp, // SP
                _ => sp,  // Fallback to SP
            };
            #[cfg(target_arch = "x86_64")]
            let base = match loc.reg {
                6 => fp, // RBP
                7 => sp, // RSP
                _ => sp,
            };
            #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
            let base = sp;

            let addr = unsafe { base.offset(loc.offset as isize) };
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
        // Clear frame info now that GC is complete
        if let Ok(mut state) = STATEPOINT_STATE.lock() {
            if let Some(ref mut s) = *state {
                s.current_frame = None;
            }
        }
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
                eprintln!("WARNING: GC block_for_gc timed out");
                return;
            }
        }
        *gc_done = false;
    }

    fn spawn_gc_thread(_tls: VMThread, ctx: GCThreadContext<StatepointVM>) {
        // Spawn a GC worker thread
        match ctx {
            GCThreadContext::Worker(worker) => {
                let handle = std::thread::spawn(move || {
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
                });

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

    // Set the plan (SemiSpace is a simple copying collector)
    builder.set_option("plan", "SemiSpace");

    // Set heap size - 64MB should be plenty for demos
    builder.options.gc_trigger.set(mmtk::util::options::GCTriggerSelector::FixedHeapSize(64 * 1024 * 1024));

    let mmtk_instance = builder.build();
    MMTK_INSTANCE.set(Box::new(mmtk_instance)).ok();

    // Initialize statepoint state
    let mut state = STATEPOINT_STATE.lock().unwrap();
    *state = Some(StatepointState {
        stackmap: None,
        code_base: 0,
        safepoint_map: std::collections::HashMap::new(),
        verbose: false,
        current_frame: None,
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
}

/// Enable collection
pub fn enable_collection() {
    let tls = VMThread::UNINITIALIZED;
    mmtk::memory_manager::initialize_collection(mmtk(), tls);
}

/// Load stackmaps from compiled code
pub fn load_stackmaps(stackmap: StackMap, code_base: usize) {
    let mut state = STATEPOINT_STATE.lock().unwrap();
    if let Some(ref mut s) = *state {
        for (idx, record) in stackmap.records.iter().enumerate() {
            s.safepoint_map.insert(record.instruction_offset, idx);
        }
        s.stackmap = Some(stackmap);
        s.code_base = code_base;
    }
}

/// Set verbose mode
pub fn set_verbose(verbose: bool) {
    let mut state = STATEPOINT_STATE.lock().unwrap();
    if let Some(ref mut s) = *state {
        s.verbose = verbose;
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
pub fn alloc_cons(car: TaggedValue, cdr: TaggedValue) -> TaggedValue {
    let size = std::mem::size_of::<ConsCell>();
    let ptr = alloc(size);

    // Set header
    let header_addr = ptr - HEAP_HEADER_SIZE;
    let header = unsafe { &mut *(header_addr.to_mut_ptr::<HeapObjectHeader>()) };
    header.forwarding = std::ptr::null_mut();
    header.size = size as u32;
    header.magic = HEAP_MAGIC;
    header.obj_type = HeapObjectType::Cons;
    header.ptr_count = 2;
    header.flags = 0;

    // Initialize cons cell
    let cons = unsafe { &mut *(ptr.to_mut_ptr::<ConsCell>()) };
    cons.car = car;
    cons.cdr = cdr;

    // Post-alloc hook for MMTk - required for proper allocation tracking
    let obj_ref = unsafe { ObjectReference::from_raw_address_unchecked(ptr) };
    MUTATOR.with(|m| {
        let mptr = m.get().expect("Mutator not bound");
        let mutator = unsafe { &mut *mptr };
        mmtk::memory_manager::post_alloc::<StatepointVM>(
            mutator,
            obj_ref,
            HEAP_HEADER_SIZE + size,
            AllocationSemantics::Default,
        );
    });

    ptr.as_usize() as TaggedValue
}

/// Trigger GC with current frame info
pub fn trigger_gc(fp: *const u8, _sp: *const u8, return_addr: usize) {
    // The stackmap locations are relative to the JIT caller's frame, not our frame.
    // On arm64: JIT's SP = our FP + 16 (above saved FP/LR pair)
    #[cfg(target_arch = "aarch64")]
    let jit_sp = (fp as usize + 16) as *const u8;
    #[cfg(target_arch = "x86_64")]
    let jit_sp = (fp as usize + 16) as *const u8;
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    let jit_sp = _sp;

    // Set current frame for root scanning
    {
        let mut state = STATEPOINT_STATE.lock().unwrap();
        if let Some(ref mut s) = *state {
            s.current_frame = Some(FrameInfo {
                fp,
                sp: jit_sp,
                return_addr,
            });
        }
    }

    // Trigger GC with force=true to ensure it actually runs
    let tls = VMMutatorThread(VMThread::UNINITIALIZED);
    mmtk().handle_user_collection_request(tls, true, false);

    // NOTE: Don't clear frame info here - it will be cleared in resume_mutators
    // after scanning is complete
}

// ============================================================================
// Runtime functions callable from JIT code
// ============================================================================

/// Runtime: Allocate a cons cell (returns raw pointer for LLVM)
#[no_mangle]
pub extern "C" fn rt_cons_raw_mmtk(car: TaggedValue, cdr: TaggedValue) -> *mut u8 {
    let result = alloc_cons(car, cdr);
    println!("  ALLOC cons at {:#x} (car={}, cdr={:#x})", result, car >> 1, cdr);
    result as *mut u8
}

/// Runtime: Trigger GC
#[no_mangle]
#[inline(never)]
pub extern "C" fn rt_gc_mmtk() {
    // Get current frame pointer
    #[cfg(target_arch = "aarch64")]
    let (fp, sp, ra) = unsafe {
        let fp: usize;
        let sp: usize;
        let ra: usize;
        std::arch::asm!(
            "mov {fp}, x29",
            "mov {sp}, sp",
            "mov {ra}, x30",
            fp = out(reg) fp,
            sp = out(reg) sp,
            ra = out(reg) ra,
            options(nomem, nostack)
        );
        (fp as *const u8, sp as *const u8, ra)
    };

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
        // Return address is at [rbp+8]
        let ra = *((fp + 8) as *const usize);
        (fp as *const u8, sp as *const u8, ra)
    };

    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    let (fp, sp, ra) = (std::ptr::null(), std::ptr::null(), 0usize);

    trigger_gc(fp, sp, ra);
}

/// Runtime car - extract first element of cons cell
#[no_mangle]
pub extern "C" fn rt_car(cell: TaggedValue) -> TaggedValue {
    let ptr = cell as usize as *const ConsCell;
    unsafe { (*ptr).car }
}

/// Runtime cdr - extract rest of cons cell
#[no_mangle]
pub extern "C" fn rt_cdr(cell: TaggedValue) -> TaggedValue {
    let ptr = cell as usize as *const ConsCell;
    unsafe { (*ptr).cdr }
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

/// Runtime symbols struct
pub struct RuntimeSymbols {
    pub cons: usize,
    pub gc: usize,
    pub car: usize,
    pub cdr: usize,
    pub print: usize,
    pub print_list: usize,
}

/// Get runtime function pointers
pub fn get_runtime_symbols() -> (usize, usize) {
    (
        rt_cons_raw_mmtk as *const () as usize,
        rt_gc_mmtk as *const () as usize,
    )
}

/// Get all runtime symbols
pub fn get_all_runtime_symbols() -> RuntimeSymbols {
    RuntimeSymbols {
        cons: rt_cons_raw_mmtk as *const () as usize,
        gc: rt_gc_mmtk as *const () as usize,
        car: rt_car as *const () as usize,
        cdr: rt_cdr as *const () as usize,
        print: rt_print as *const () as usize,
        print_list: rt_print_list as *const () as usize,
    }
}

/// Get allocator mapping for fast-path allocation
pub fn get_allocator_mapping() -> AllocatorSelector {
    mmtk::memory_manager::get_allocator_mapping(
        mmtk(),
        AllocationSemantics::Default,
    )
}
