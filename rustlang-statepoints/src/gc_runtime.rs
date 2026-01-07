//! Simple GC Runtime with Stackmap-based Root Scanning
//!
//! This is a lightweight alternative to MMTk for simple benchmarks.
//! Uses a semi-space copying collector with proper stack walking.

use std::alloc::{alloc, dealloc, Layout};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ptr;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::tagged_value::*;
use crate::stackmap::{StackMap, Location, LocationType};

// Stack walker library for cross-platform frame pointer chain walking
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

/// Size of each semi-space (default 16MB, configurable via GC_HEAP_MB)
fn heap_size() -> usize {
    std::env::var("GC_HEAP_MB")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(16) * 1024 * 1024
}

/// Global GC instance
thread_local! {
    static GC: RefCell<Option<GarbageCollector>> = const { RefCell::new(None) };
}

/// Semi-space copying garbage collector
pub struct GarbageCollector {
    /// From-space: where we allocate
    from_space: *mut u8,
    /// To-space: where we copy during GC
    to_space: *mut u8,
    /// Current allocation pointer in from-space
    alloc_ptr: *mut u8,
    /// End of from-space
    from_end: *mut u8,
    /// Scan pointer during GC (Cheney's algorithm)
    scan_ptr: *mut u8,
    /// Free pointer in to-space during GC
    free_ptr: *mut u8,
    /// Heap size
    heap_size: usize,
    /// Statistics
    pub gc_count: usize,
    pub bytes_allocated: usize,
    pub bytes_copied: usize,
    /// Verbose output
    verbose: bool,
    /// Parsed stackmaps (loaded after JIT compilation)
    _stackmaps: Option<StackMap>,
    /// Global root for keeping objects alive
    global_root: TaggedValue,
}

impl GarbageCollector {
    pub fn new() -> Self {
        let heap_size = heap_size();
        let layout = Layout::from_size_align(heap_size, 8).unwrap();

        let from_space = unsafe { alloc(layout) };
        let to_space = unsafe { alloc(layout) };

        if from_space.is_null() || to_space.is_null() {
            panic!("Failed to allocate heap");
        }

        eprintln!("Simple GC initialized: {}MB heap", heap_size / 1024 / 1024);

        GarbageCollector {
            from_space,
            to_space,
            alloc_ptr: from_space,
            from_end: unsafe { from_space.add(heap_size) },
            scan_ptr: ptr::null_mut(),
            free_ptr: ptr::null_mut(),
            heap_size,
            gc_count: 0,
            bytes_allocated: 0,
            bytes_copied: 0,
            verbose: std::env::var("GC_TRACE").is_ok(),
            _stackmaps: None,
            global_root: 0,
        }
    }

    /// Load stackmaps from compiled code
    pub fn load_stackmaps(&mut self, stackmap: StackMap) {
        if self.verbose {
            println!("  [GC] Loaded {} stackmap records", stackmap.records.len());
        }
        self._stackmaps = Some(stackmap);
    }

    /// Check if we have space for a cons cell (without triggering GC)
    pub fn has_space_for_cons(&self) -> bool {
        let size = std::mem::size_of::<ConsCell>();
        let aligned_size = (size + 7) & !7;
        let total_size = HEAP_HEADER_SIZE + aligned_size;
        let new_alloc_ptr = unsafe { self.alloc_ptr.add(total_size) };
        new_alloc_ptr <= self.from_end
    }

    /// Allocate a cons cell WITHOUT triggering GC (caller must check has_space first)
    pub fn alloc_cons_no_gc(&mut self, car: TaggedValue, cdr: TaggedValue) -> *mut u8 {
        let size = std::mem::size_of::<ConsCell>();
        let aligned_size = (size + 7) & !7;
        let total_size = HEAP_HEADER_SIZE + aligned_size;

        // Set up header
        let header_ptr = self.alloc_ptr as *mut HeapObjectHeader;
        unsafe {
            (*header_ptr).forwarding = ptr::null_mut();
            (*header_ptr).size = aligned_size as u32;
            (*header_ptr).magic = HEAP_MAGIC;
            (*header_ptr).obj_type = HeapObjectType::Cons;
            (*header_ptr).ptr_count = 2;
            (*header_ptr).flags = 0;
        }

        let data_ptr = unsafe { self.alloc_ptr.add(HEAP_HEADER_SIZE) };
        self.alloc_ptr = unsafe { self.alloc_ptr.add(total_size) };
        self.bytes_allocated += total_size;

        // Initialize cons cell
        unsafe {
            let cons = data_ptr as *mut ConsCell;
            (*cons).car = car;
            (*cons).cdr = cdr;
        }

        data_ptr
    }

    /// Trigger collection (public for JIT to call at safepoint)
    pub fn collect(&mut self) {
        self.do_collect(&[]);
    }

    /// Collect with explicit root slots from JIT code
    pub fn collect_with_explicit_roots(&mut self, root1: *mut TaggedValue, root2: *mut TaggedValue) {
        let mut roots = Vec::new();
        if !root1.is_null() {
            roots.push(root1);
        }
        if !root2.is_null() {
            roots.push(root2);
        }
        self.do_collect(&roots);
    }

    /// Collect with stack walking using stackmap
    /// Now uses the stack-walker library for cross-platform FP chain traversal
    pub fn collect_with_stack_walk(&mut self, fp: usize, sp: usize, ra: usize) {
        self.gc_count += 1;

        if self.verbose {
            eprintln!("[GC] Collection #{} with stack walk, fp={:#x} sp={:#x} ra={:#x}",
                self.gc_count, fp, sp, ra);
        }

        // Initialize to-space
        self.scan_ptr = self.to_space;
        self.free_ptr = self.to_space;
        self.bytes_copied = 0;

        // Process global root
        if is_heap_ptr(self.global_root) {
            let old_ptr = self.global_root as *mut u8;
            let new_ptr = self.copy_object(old_ptr);
            self.global_root = new_ptr as TaggedValue;
        }

        // Walk the stack and find roots using stackmap
        let stack_low = STACK_LOW.load(Ordering::Acquire);
        let stack_high = STACK_HIGH.load(Ordering::Acquire);

        STACKMAP.with(|sm_cell| {
            SAFEPOINT_MAP.with(|sp_map_cell| {
                let sm_ref = sm_cell.borrow();
                let sp_map = sp_map_cell.borrow();

                if let Some(ref stackmap) = *sm_ref {
                    // Use stack-walker library for frame pointer chain traversal
                    #[cfg(target_arch = "aarch64")]
                    let walker = Aarch64StackWalker::apple_silicon();
                    #[cfg(target_arch = "x86_64")]
                    let walker = X86_64StackWalker::new();

                    #[cfg(target_arch = "aarch64")]
                    let regs = UnwindRegsAarch64::new(ra as u64, sp as u64, fp as u64, 0);
                    #[cfg(target_arch = "x86_64")]
                    let regs = UnwindRegsX86_64::new(ra as u64, sp as u64, fp as u64);

                    let mut reader = UnsafeDirectReader::new();
                    let config = WalkConfig {
                        max_frames: 100,
                        validate_return_addresses: false, // We validate via stackmap lookup
                        ..Default::default()
                    };

                    let verbose = self.verbose;
                    let mut frame_depth = 0usize;

                    walker.walk_with(&regs, &mut reader, &config, |frame| {
                        frame_depth += 1;
                        let current_fp = frame.frame_pointer.unwrap_or(0) as usize;
                        let current_sp = frame.stack_pointer as usize;
                        let current_ra = frame.raw_address() as usize;

                        // Stop if outside stack bounds
                        if current_fp != 0 && (current_fp < stack_low || current_fp >= stack_high) {
                            return false;
                        }

                        // Look up safepoint record for this return address
                        if let Some(&record_idx) = sp_map.get(&(current_ra as u64)) {
                            let record = &stackmap.records[record_idx];
                            let gc_locs = stackmap.get_gc_locations(record);

                            if verbose {
                                eprintln!("  Frame {}: ra={:#x} has {} GC locs",
                                    frame_depth, current_ra, gc_locs.len());
                            }

                            // Process each GC location
                            for (_base_loc, derived_loc) in &gc_locs {
                                if let Some(addr) = resolve_location_static(
                                    derived_loc, current_fp, current_sp, stack_low, stack_high
                                ) {
                                    let slot = addr as *mut TaggedValue;
                                    let val = unsafe { *slot };
                                    if is_heap_ptr(val) {
                                        let old_ptr = val as *mut u8;
                                        let new_ptr = self.copy_object(old_ptr);
                                        unsafe { *slot = new_ptr as TaggedValue };
                                        if verbose {
                                            eprintln!("    Root at {:#x}: {:#x} -> {:#x}",
                                                addr, old_ptr as usize, new_ptr as usize);
                                        }
                                    }
                                }
                            }
                        }

                        // Check if this address is in a known JIT function
                        let in_jit = stackmap.functions.iter().any(|f| {
                            (current_ra as u64) >= f.address &&
                            (current_ra as u64) < f.address + f.stack_size as u64
                        });

                        in_jit // Continue walking only if in JIT code
                    });
                }
            });
        });

        // Cheney's scan
        while self.scan_ptr < self.free_ptr {
            let header_ptr = self.scan_ptr as *mut HeapObjectHeader;
            let header = unsafe { &*header_ptr };
            let data_ptr = unsafe { self.scan_ptr.add(HEAP_HEADER_SIZE) };
            let total_size = HEAP_HEADER_SIZE + header.size as usize;

            self.scan_object(data_ptr);
            self.scan_ptr = unsafe { self.scan_ptr.add(total_size) };
        }

        // Swap spaces
        std::mem::swap(&mut self.from_space, &mut self.to_space);
        self.alloc_ptr = self.free_ptr;
        self.from_end = unsafe { self.from_space.add(self.heap_size) };

        if self.verbose {
            eprintln!("[GC] Collection complete: {} bytes copied", self.bytes_copied);
        }
    }

    /// Resolve a stackmap location to a stack address
    #[allow(dead_code)]
    fn resolve_location(
        &self,
        loc: &Location,
        fp: usize,
        sp: usize,
        stack_low: usize,
        stack_high: usize,
    ) -> Option<usize> {
        resolve_location_static(loc, fp, sp, stack_low, stack_high)
    }

    /// Internal collection implementation
    fn do_collect(&mut self, explicit_roots: &[*mut TaggedValue]) {
        self.gc_count += 1;

        if self.verbose {
            eprintln!("[GC] Collection #{} starting", self.gc_count);
        }

        // Initialize to-space
        self.scan_ptr = self.to_space;
        self.free_ptr = self.to_space;
        self.bytes_copied = 0;

        // Process global root
        if is_heap_ptr(self.global_root) {
            let old_ptr = self.global_root as *mut u8;
            let new_ptr = self.copy_object(old_ptr);
            self.global_root = new_ptr as TaggedValue;
        }

        // Process explicit roots from JIT stack slots
        for root_ptr in explicit_roots {
            let val = unsafe { **root_ptr };
            if is_heap_ptr(val) {
                let old_ptr = val as *mut u8;
                let new_ptr = self.copy_object(old_ptr);
                unsafe { **root_ptr = new_ptr as TaggedValue };
            }
        }

        // Cheney's scan
        while self.scan_ptr < self.free_ptr {
            let header_ptr = self.scan_ptr as *mut HeapObjectHeader;
            let header = unsafe { &*header_ptr };
            let data_ptr = unsafe { self.scan_ptr.add(HEAP_HEADER_SIZE) };
            let total_size = HEAP_HEADER_SIZE + header.size as usize;

            self.scan_object(data_ptr);
            self.scan_ptr = unsafe { self.scan_ptr.add(total_size) };
        }

        // Swap spaces
        std::mem::swap(&mut self.from_space, &mut self.to_space);
        self.alloc_ptr = self.free_ptr;
        self.from_end = unsafe { self.from_space.add(self.heap_size) };

        if self.verbose {
            eprintln!("[GC] Collection complete: {} bytes copied", self.bytes_copied);
        }
    }

    /// Check if a pointer is in from-space
    fn in_from_space(&self, ptr: *mut u8) -> bool {
        ptr >= self.from_space && ptr < self.from_end
    }

    /// Copy an object to to-space and leave forwarding pointer
    fn copy_object(&mut self, old_ptr: *mut u8) -> *mut u8 {
        if old_ptr.is_null() || !self.in_from_space(old_ptr) {
            return old_ptr;
        }

        let header_ptr = unsafe { old_ptr.sub(HEAP_HEADER_SIZE) as *mut HeapObjectHeader };
        let header = unsafe { &mut *header_ptr };

        // Check if already forwarded
        if !header.forwarding.is_null() {
            return header.forwarding;
        }

        // Copy to to-space
        let total_size = HEAP_HEADER_SIZE + header.size as usize;
        let new_header_ptr = self.free_ptr;
        let new_data_ptr = unsafe { new_header_ptr.add(HEAP_HEADER_SIZE) };

        unsafe {
            ptr::copy_nonoverlapping(header_ptr as *const u8, new_header_ptr, total_size);
        }

        self.free_ptr = unsafe { self.free_ptr.add(total_size) };
        self.bytes_copied += total_size;

        // Clear forwarding pointer in new location
        unsafe {
            let new_header = new_header_ptr as *mut HeapObjectHeader;
            (*new_header).forwarding = ptr::null_mut();
        }

        // Set forwarding pointer in old location
        header.forwarding = new_data_ptr;

        new_data_ptr
    }

    /// Scan an object and copy any referenced objects
    fn scan_object(&mut self, ptr: *mut u8) {
        let header_ptr = unsafe { ptr.sub(HEAP_HEADER_SIZE) as *mut HeapObjectHeader };
        let header = unsafe { &*header_ptr };

        match header.obj_type {
            HeapObjectType::Cons => {
                let cons = ptr as *mut ConsCell;
                unsafe {
                    if is_heap_ptr((*cons).car) {
                        let old_ptr = (*cons).car as *mut u8;
                        let new_ptr = self.copy_object(old_ptr);
                        (*cons).car = new_ptr as TaggedValue;
                    }
                    if is_heap_ptr((*cons).cdr) {
                        let old_ptr = (*cons).cdr as *mut u8;
                        let new_ptr = self.copy_object(old_ptr);
                        (*cons).cdr = new_ptr as TaggedValue;
                    }
                }
            }
        }
    }

    pub fn set_global_root(&mut self, val: TaggedValue) {
        self.global_root = val;
    }

    pub fn clear_global_root(&mut self) {
        self.global_root = 0;
    }
}

impl Drop for GarbageCollector {
    fn drop(&mut self) {
        let layout = Layout::from_size_align(self.heap_size, 8).unwrap();
        unsafe {
            dealloc(self.from_space, layout);
            dealloc(self.to_space, layout);
        }
    }
}

/// Resolve a stackmap location to a stack address (standalone version)
/// Used from within closures where self is already borrowed
fn resolve_location_static(
    loc: &Location,
    fp: usize,
    sp: usize,
    stack_low: usize,
    stack_high: usize,
) -> Option<usize> {
    if loc.ty == LocationType::Register {
        // Register locations - we don't have saved registers in this simple GC
        return None;
    }

    #[cfg(target_arch = "aarch64")]
    let base = match loc.reg {
        29 => fp,  // x29 = FP
        31 => sp,  // x31 = SP
        _ => return None,
    };
    #[cfg(target_arch = "x86_64")]
    let base = match loc.reg {
        6 => fp,  // RBP
        7 => sp,  // RSP
        _ => return None,
    };
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    let base = sp;

    let addr = if loc.offset >= 0 {
        base.checked_add(loc.offset as usize)?
    } else {
        base.checked_sub((-loc.offset) as usize)?
    };

    // Validate address is within stack bounds
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
        *gc.borrow_mut() = Some(GarbageCollector::new());
    });
}

/// Load stackmaps and build safepoint lookup table
pub fn load_stackmaps(stackmap: StackMap, _main_fn: usize) {
    // Build safepoint map: return_address -> record_index
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

    // Set up stack bounds
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
        // Fallback: use reasonable defaults
        STACK_LOW.store(0, Ordering::Release);
        STACK_HIGH.store(usize::MAX, Ordering::Release);
    }
}

// ============================================================================
// Runtime functions callable from JIT code
// ============================================================================

/// Try to allocate a cons cell - returns NULL if heap is full (caller should GC and retry)
#[no_mangle]
pub extern "C" fn rt_try_alloc_cons(car: TaggedValue, cdr: TaggedValue) -> *mut u8 {
    GC.with(|gc| {
        let mut gc = gc.borrow_mut();
        let collector = gc.as_mut().expect("GC not initialized");

        // Check if we have space (don't trigger GC here!)
        if !collector.has_space_for_cons() {
            return std::ptr::null_mut();  // Signal: need GC
        }

        // Allocate without GC
        collector.alloc_cons_no_gc(car, cdr)
    })
}

/// Trigger GC with frame info for stack walking
#[no_mangle]
pub extern "C" fn rt_gc_with_frame_info(fp: usize, sp: usize, ra: usize) {
    GC.with(|gc| {
        if let Some(ref mut collector) = *gc.borrow_mut() {
            collector.collect_with_stack_walk(fp, sp, ra);
        }
    });
}

/// Trigger GC with explicit root slots (passed from JIT code) - legacy
#[no_mangle]
pub extern "C" fn rt_gc_with_roots(root1: *mut TaggedValue, root2: *mut TaggedValue) {
    GC.with(|gc| {
        if let Some(ref mut collector) = *gc.borrow_mut() {
            collector.collect_with_explicit_roots(root1, root2);
        }
    });
}

#[no_mangle]
pub extern "C" fn rt_gc_simple() {
    GC.with(|gc| {
        if let Some(ref mut collector) = *gc.borrow_mut() {
            collector.collect();
        }
    });
}

#[no_mangle]
pub extern "C" fn rt_car_simple(cell: TaggedValue) -> TaggedValue {
    let ptr = cell as usize as *const ConsCell;
    unsafe { (*ptr).car }
}

#[no_mangle]
pub extern "C" fn rt_cdr_simple(cell: TaggedValue) -> TaggedValue {
    let ptr = cell as usize as *const ConsCell;
    unsafe { (*ptr).cdr }
}

#[no_mangle]
pub extern "C" fn rt_set_global_root_simple(val: TaggedValue) {
    GC.with(|gc| {
        if let Some(ref mut collector) = *gc.borrow_mut() {
            collector.set_global_root(val);
        }
    });
}

#[no_mangle]
pub extern "C" fn rt_clear_global_root_simple() {
    GC.with(|gc| {
        if let Some(ref mut collector) = *gc.borrow_mut() {
            collector.clear_global_root();
        }
    });
}

/// Runtime symbols struct for simple GC
pub struct SimpleRuntimeSymbols {
    pub try_alloc_cons: usize,
    pub gc: usize,
    pub gc_with_roots: usize,
    pub gc_with_frame_info: usize,
    pub car: usize,
    pub cdr: usize,
    pub set_global_root: usize,
    pub clear_global_root: usize,
}

pub fn get_simple_runtime_symbols() -> SimpleRuntimeSymbols {
    SimpleRuntimeSymbols {
        try_alloc_cons: rt_try_alloc_cons as *const () as usize,
        gc: rt_gc_simple as *const () as usize,
        gc_with_roots: rt_gc_with_roots as *const () as usize,
        gc_with_frame_info: rt_gc_with_frame_info as *const () as usize,
        car: rt_car_simple as *const () as usize,
        cdr: rt_cdr_simple as *const () as usize,
        set_global_root: rt_set_global_root_simple as *const () as usize,
        clear_global_root: rt_clear_global_root_simple as *const () as usize,
    }
}
