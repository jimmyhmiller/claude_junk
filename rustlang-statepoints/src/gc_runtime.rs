//! GC Runtime with Stackmap-based Root Scanning
//!
//! This GC uses LLVM's statepoint infrastructure instead of a runtime shadow stack.
//! At safepoints, LLVM generates stackmaps that tell us exactly where live GC pointers
//! are on the stack. The GC uses these stackmaps to find and update roots.
//!
//! Key design:
//! - No runtime shadow stack overhead
//! - LLVM tracks `ptr addrspace(1)` values automatically
//! - Stackmaps tell us where pointers live at each safepoint
//! - Semi-space copying collector moves objects and updates stack slots

use std::alloc::{alloc, dealloc, Layout};
use std::cell::RefCell;
use std::ptr;

use crate::tagged_value::*;
use crate::stackmap::{StackMap, StackMapRecord, Location, LocationType};

/// Size of each semi-space (1MB)
const HEAP_SIZE: usize = 1024 * 1024;

/// Global GC instance
thread_local! {
    static GC: RefCell<Option<GarbageCollector>> = RefCell::new(None);
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
    /// Statistics
    pub gc_count: usize,
    pub bytes_allocated: usize,
    pub bytes_copied: usize,
    /// Verbose output
    verbose: bool,
    /// Parsed stackmaps (loaded after JIT compilation)
    stackmaps: Option<StackMap>,
}

impl GarbageCollector {
    pub fn new() -> Self {
        let layout = Layout::from_size_align(HEAP_SIZE, 8).unwrap();

        let from_space = unsafe { alloc(layout) };
        let to_space = unsafe { alloc(layout) };

        if from_space.is_null() || to_space.is_null() {
            panic!("Failed to allocate heap");
        }

        // Fill with recognizable patterns for debugging
        unsafe {
            ptr::write_bytes(from_space, 0xAA, HEAP_SIZE);
            ptr::write_bytes(to_space, 0xBB, HEAP_SIZE);
        }

        GarbageCollector {
            from_space,
            to_space,
            alloc_ptr: from_space,
            from_end: unsafe { from_space.add(HEAP_SIZE) },
            scan_ptr: ptr::null_mut(),
            free_ptr: ptr::null_mut(),
            gc_count: 0,
            bytes_allocated: 0,
            bytes_copied: 0,
            verbose: false,
            stackmaps: None,
        }
    }

    /// Load stackmaps from compiled code
    pub fn load_stackmaps(&mut self, stackmap: StackMap) {
        if self.verbose {
            println!("  [GC] Loaded {} stackmap records", stackmap.records.len());
        }
        self.stackmaps = Some(stackmap);
    }

    /// Allocate memory for an object
    pub fn alloc(&mut self, size: usize) -> *mut u8 {
        // Round up to 8-byte alignment
        let aligned_size = (size + 7) & !7;
        let total_size = HEAP_HEADER_SIZE + aligned_size;

        let new_alloc_ptr = unsafe { self.alloc_ptr.add(total_size) };

        if new_alloc_ptr > self.from_end {
            // Out of memory - would need GC, but for now panic
            panic!("Out of heap memory! Need to trigger GC.");
        }

        // Set up header
        let header_ptr = self.alloc_ptr as *mut HeapObjectHeader;
        unsafe {
            (*header_ptr).forwarding = ptr::null_mut();
            (*header_ptr).size = aligned_size as u32;
            (*header_ptr).magic = HEAP_MAGIC;
            (*header_ptr).obj_type = HeapObjectType::Cons; // default
            (*header_ptr).ptr_count = 0;
            (*header_ptr).flags = 0;
        }

        // Return pointer to data (after header)
        let data_ptr = unsafe { self.alloc_ptr.add(HEAP_HEADER_SIZE) };
        self.alloc_ptr = new_alloc_ptr;
        self.bytes_allocated += total_size;

        data_ptr
    }

    /// Allocate a cons cell
    pub fn alloc_cons(&mut self, car: TaggedValue, cdr: TaggedValue) -> TaggedValue {
        let size = std::mem::size_of::<ConsCell>();
        let ptr = self.alloc(size);

        // Set header info
        unsafe {
            let header = (ptr as *mut u8).sub(HEAP_HEADER_SIZE) as *mut HeapObjectHeader;
            (*header).obj_type = HeapObjectType::Cons;
            (*header).ptr_count = 2; // car and cdr are both potentially pointers

            // Initialize cons cell
            let cons = ptr as *mut ConsCell;
            (*cons).car = car;
            (*cons).cdr = cdr;
        }

        if self.verbose {
            println!("  [GC] Allocated cons at {:#x}", ptr as usize);
        }

        ptr as TaggedValue
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

        // Get header (before data pointer)
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

        // Update free pointer
        self.free_ptr = unsafe { self.free_ptr.add(total_size) };
        self.bytes_copied += total_size;

        // Clear forwarding pointer in new location
        unsafe {
            let new_header = new_header_ptr as *mut HeapObjectHeader;
            (*new_header).forwarding = ptr::null_mut();
        }

        // Set forwarding pointer in old location
        header.forwarding = new_data_ptr;

        if self.verbose {
            println!("  [GC] Copied object {:#x} -> {:#x}", old_ptr as usize, new_data_ptr as usize);
        }

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
                    // Process car
                    if is_heap_ptr((*cons).car) {
                        let old_ptr = (*cons).car as *mut u8;
                        let new_ptr = self.copy_object(old_ptr);
                        (*cons).car = new_ptr as TaggedValue;
                    }
                    // Process cdr
                    if is_heap_ptr((*cons).cdr) {
                        let old_ptr = (*cons).cdr as *mut u8;
                        let new_ptr = self.copy_object(old_ptr);
                        (*cons).cdr = new_ptr as TaggedValue;
                    }
                }
            }
            _ => {
                // Other object types would be handled here
            }
        }
    }

    /// Collect garbage using roots passed directly (for simple cases)
    pub fn collect_with_roots(&mut self, roots: &mut [*mut TaggedValue]) {
        self.gc_count += 1;

        if self.verbose {
            println!("\n  [GC] Collection #{} starting", self.gc_count);
            println!("  [GC] From-space: {:#x}", self.from_space as usize);
            println!("  [GC] To-space: {:#x}", self.to_space as usize);
        }

        // Initialize to-space
        self.scan_ptr = self.to_space;
        self.free_ptr = self.to_space;
        self.bytes_copied = 0;

        // Process roots
        for root in roots.iter_mut() {
            let value = unsafe { **root };
            if is_heap_ptr(value) {
                let old_ptr = value as *mut u8;
                let new_ptr = self.copy_object(old_ptr);
                unsafe { **root = new_ptr as TaggedValue };

                if self.verbose {
                    println!("  [GC] Root updated: {:#x} -> {:#x}", old_ptr as usize, new_ptr as usize);
                }
            }
        }

        // Cheney's scan - process objects in to-space breadth-first
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
        self.from_end = unsafe { self.from_space.add(HEAP_SIZE) };

        // Poison old space for debugging
        unsafe {
            ptr::write_bytes(self.to_space, 0xDD, HEAP_SIZE);
        }

        if self.verbose {
            println!("  [GC] Collection complete: {} bytes copied", self.bytes_copied);
            println!("  [GC] New from-space: {:#x}", self.from_space as usize);
        }
    }

    /// Collect using stackmap-based root scanning
    ///
    /// This is the key function that uses LLVM stackmaps to find GC roots.
    /// At a safepoint, we:
    /// 1. Look up the stackmap record for the current instruction
    /// 2. For each GC pointer location in the record, read the pointer from the stack
    /// 3. Copy the object and update the stack slot with the new address
    pub fn collect_with_stackmap(
        &mut self,
        rbp: *const u8,
        rsp: *const u8,
        record: &StackMapRecord,
    ) {
        self.gc_count += 1;

        if self.verbose {
            println!("\n  [GC] Stackmap-based collection #{}", self.gc_count);
            println!("  [GC] RBP={:#x}, RSP={:#x}", rbp as usize, rsp as usize);
        }

        // Initialize to-space
        self.scan_ptr = self.to_space;
        self.free_ptr = self.to_space;
        self.bytes_copied = 0;

        // Get GC pointer locations from stackmap
        let stackmap = self.stackmaps.as_ref().expect("No stackmaps loaded");
        let gc_locs = stackmap.get_gc_locations(record);

        if self.verbose {
            println!("  [GC] Found {} GC pointer pairs in stackmap", gc_locs.len());
        }

        // Process each GC pointer location
        for (base_loc, derived_loc) in &gc_locs {
            // For now, we only handle the simple case where base == derived
            // (no interior pointers)
            let slot_ptr = self.resolve_location(derived_loc, rbp, rsp);

            if let Some(slot) = slot_ptr {
                let value = unsafe { *(slot as *const TaggedValue) };

                if is_heap_ptr(value) {
                    let old_ptr = value as *mut u8;
                    let new_ptr = self.copy_object(old_ptr);
                    unsafe { *(slot as *mut TaggedValue) = new_ptr as TaggedValue };

                    if self.verbose {
                        println!("  [GC] Stackmap root {} updated: {:#x} -> {:#x}",
                                 derived_loc, old_ptr as usize, new_ptr as usize);
                    }
                }
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
        self.from_end = unsafe { self.from_space.add(HEAP_SIZE) };

        // Poison old space
        unsafe {
            ptr::write_bytes(self.to_space, 0xDD, HEAP_SIZE);
        }

        if self.verbose {
            println!("  [GC] Collection complete: {} bytes copied", self.bytes_copied);
        }
    }

    /// Resolve a stackmap location to a stack address
    fn resolve_location(&self, loc: &Location, rbp: *const u8, rsp: *const u8) -> Option<*mut u8> {
        match loc.ty {
            LocationType::Indirect => {
                // Indirect: value is at [reg + offset]
                let base = match loc.reg {
                    6 => rbp,  // RBP
                    7 => rsp,  // RSP
                    _ => {
                        if self.verbose {
                            println!("  [GC] Unsupported register in location: {}", loc.reg);
                        }
                        return None;
                    }
                };
                let addr = unsafe { base.offset(loc.offset as isize) };
                Some(addr as *mut u8)
            }
            LocationType::Register => {
                // Register location - would need register context
                // For now, we expect LLVM to spill to stack at safepoints
                if self.verbose {
                    println!("  [GC] Register location not yet supported: {}", loc);
                }
                None
            }
            _ => {
                if self.verbose {
                    println!("  [GC] Unsupported location type: {:?}", loc.ty);
                }
                None
            }
        }
    }

    pub fn set_verbose(&mut self, v: bool) {
        self.verbose = v;
    }

    pub fn stats(&self) {
        println!("\n─── GC Statistics ───");
        println!("  Collections: {}", self.gc_count);
        println!("  Bytes allocated: {}", self.bytes_allocated);
        println!("  Bytes in last copy: {}", self.bytes_copied);
        println!("  From-space: {:#x}", self.from_space as usize);
        println!("  Alloc ptr: {:#x}", self.alloc_ptr as usize);
        if let Some(ref sm) = self.stackmaps {
            println!("  Stackmap records: {}", sm.records.len());
        }
    }
}

impl Drop for GarbageCollector {
    fn drop(&mut self) {
        let layout = Layout::from_size_align(HEAP_SIZE, 8).unwrap();
        unsafe {
            dealloc(self.from_space, layout);
            dealloc(self.to_space, layout);
        }
    }
}

// ============================================================================
// Public API
// ============================================================================

/// Initialize the GC
pub fn init() {
    GC.with(|gc| {
        *gc.borrow_mut() = Some(GarbageCollector::new());
    });
}

/// Reset the GC (for testing)
pub fn reset() {
    GC.with(|gc| {
        *gc.borrow_mut() = Some(GarbageCollector::new());
    });
}

/// Set verbose mode
pub fn set_verbose(v: bool) {
    GC.with(|gc| {
        if let Some(ref mut collector) = *gc.borrow_mut() {
            collector.set_verbose(v);
        }
    });
}

/// Load stackmaps
pub fn load_stackmaps(stackmap: StackMap) {
    GC.with(|gc| {
        if let Some(ref mut collector) = *gc.borrow_mut() {
            collector.load_stackmaps(stackmap);
        }
    });
}

/// Allocate a cons cell
pub fn cons(car: TaggedValue, cdr: TaggedValue) -> TaggedValue {
    GC.with(|gc| {
        gc.borrow_mut()
            .as_mut()
            .expect("GC not initialized")
            .alloc_cons(car, cdr)
    })
}

/// Trigger garbage collection with explicit roots
pub fn collect_with_roots(roots: &mut [*mut TaggedValue]) {
    GC.with(|gc| {
        if let Some(ref mut collector) = *gc.borrow_mut() {
            collector.collect_with_roots(roots);
        }
    });
}

/// Print GC statistics
pub fn stats() {
    GC.with(|gc| {
        if let Some(ref collector) = *gc.borrow() {
            collector.stats();
        }
    });
}

// ============================================================================
// Runtime functions callable from JIT code
// ============================================================================

/// Runtime: Allocate a cons cell
/// Returns raw pointer (not tagged) for LLVM to track as ptr addrspace(1)
#[no_mangle]
pub extern "C" fn rt_cons_raw(car: TaggedValue, cdr: TaggedValue) -> *mut u8 {
    GC.with(|gc| {
        let result = gc.borrow_mut()
            .as_mut()
            .expect("GC not initialized")
            .alloc_cons(car, cdr);
        result as *mut u8
    })
}

/// Runtime: Allocate a cons cell (tagged version for compatibility)
#[no_mangle]
pub extern "C" fn rt_cons(car: TaggedValue, cdr: TaggedValue) -> TaggedValue {
    cons(car, cdr)
}

/// Runtime: Trigger GC with explicit roots
/// This is called when we need to collect but don't have stackmap info
#[no_mangle]
pub extern "C" fn rt_gc_with_roots(roots: *mut *mut TaggedValue, count: usize) {
    let root_slice = unsafe { std::slice::from_raw_parts_mut(roots, count) };
    collect_with_roots(root_slice);
}

/// Runtime: Trigger GC (placeholder - needs stack walking)
#[no_mangle]
pub extern "C" fn rt_gc() {
    // In a real implementation, this would:
    // 1. Walk the stack to find return addresses
    // 2. Look up stackmap records for each return address
    // 3. Use the stackmap to find live pointers
    // 4. Call collect_with_stackmap
    //
    // For now, this is a no-op since we pass roots explicitly
    println!("  [GC] rt_gc called (stack walking not implemented yet)");
}
