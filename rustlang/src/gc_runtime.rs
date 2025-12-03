//! Simple semi-space copying garbage collector runtime
//!
//! This is a proof-of-concept to demonstrate LLVM statepoints working correctly.
//! Objects are allocated in one semi-space, and when GC triggers, live objects
//! are copied to the other semi-space, updating all pointers.

use std::alloc::{alloc, dealloc, Layout};
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// Size of each semi-space (4MB for demo)
const SEMI_SPACE_SIZE: usize = 4 * 1024 * 1024;

/// Object header that precedes every GC-managed object
#[repr(C)]
pub struct ObjectHeader {
    /// Size of the object (not including header)
    pub size: usize,
    /// Forwarding pointer (used during GC to point to new location)
    pub forwarding: *mut u8,
    /// Magic number for debugging
    pub magic: u32,
}

const OBJECT_MAGIC: u32 = 0xDEAD_BEEF;
const HEADER_SIZE: usize = std::mem::size_of::<ObjectHeader>();

/// The global GC state
pub struct GcState {
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

    /// Number of GCs performed
    pub gc_count: AtomicUsize,
    /// Number of objects allocated
    pub alloc_count: AtomicUsize,
    /// Number of objects moved
    pub move_count: AtomicUsize,

    /// Flag to force GC on next allocation (for testing)
    pub force_gc: AtomicBool,
}

impl GcState {
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
            println!("[GC] Space A: {:p}", space_a);
            println!("[GC] Space B: {:p}", space_b);

            GcState {
                space_a,
                space_b,
                from_space: space_a,
                to_space: space_b,
                alloc_ptr: AtomicUsize::new(space_a as usize),
                from_space_end: space_a as usize + SEMI_SPACE_SIZE,
                gc_count: AtomicUsize::new(0),
                alloc_count: AtomicUsize::new(0),
                move_count: AtomicUsize::new(0),
                force_gc: AtomicBool::new(false),
            }
        }
    }

    /// Allocate an object of the given size
    /// Returns a pointer to the object data (after the header)
    pub fn alloc(&self, size: usize) -> *mut u8 {
        // Align size to 8 bytes
        let aligned_size = (size + 7) & !7;
        let total_size = HEADER_SIZE + aligned_size;

        // Try to bump allocate
        loop {
            let current = self.alloc_ptr.load(Ordering::Relaxed);
            let new_ptr = current + total_size;

            if new_ptr > self.from_space_end || self.force_gc.load(Ordering::Relaxed) {
                // Out of space - would trigger GC in real implementation
                // For now, just report and continue (we'll handle GC separately)
                println!("[GC] Allocation of {} bytes (need GC or more space)", size);

                // For demo, just reset and continue (not realistic, but shows the flow)
                if self.force_gc.swap(false, Ordering::Relaxed) {
                    println!("[GC] Force GC flag was set, continuing...");
                }
            }

            if self.alloc_ptr.compare_exchange_weak(
                current, new_ptr, Ordering::SeqCst, Ordering::Relaxed
            ).is_ok() {
                let header_ptr = current as *mut ObjectHeader;
                let data_ptr = (current + HEADER_SIZE) as *mut u8;

                unsafe {
                    (*header_ptr).size = aligned_size;
                    (*header_ptr).forwarding = ptr::null_mut();
                    (*header_ptr).magic = OBJECT_MAGIC;

                    // Zero the object data
                    ptr::write_bytes(data_ptr, 0, aligned_size);
                }

                self.alloc_count.fetch_add(1, Ordering::Relaxed);

                println!("[GC] Allocated {} bytes at {:p} (object #{})",
                    size, data_ptr, self.alloc_count.load(Ordering::Relaxed));

                return data_ptr;
            }
        }
    }

    /// Check if a pointer is in the from-space
    pub fn is_gc_ptr(&self, ptr: *mut u8) -> bool {
        let addr = ptr as usize;
        let from_start = self.from_space as usize;
        let from_end = from_start + SEMI_SPACE_SIZE;
        addr >= from_start && addr < from_end
    }

    /// Get the header for an object pointer
    unsafe fn get_header(ptr: *mut u8) -> *mut ObjectHeader {
        (ptr as usize - HEADER_SIZE) as *mut ObjectHeader
    }

    /// Copy an object to to-space and return the new address
    /// If already copied (has forwarding pointer), return that
    pub unsafe fn copy_object(&self, ptr: *mut u8, to_alloc: &mut *mut u8) -> *mut u8 {
        if ptr.is_null() || !self.is_gc_ptr(ptr) {
            return ptr;
        }

        let header = Self::get_header(ptr);

        // Check if already forwarded
        if !(*header).forwarding.is_null() {
            return (*header).forwarding;
        }

        // Copy to to-space
        let size = (*header).size;
        let total_size = HEADER_SIZE + size;

        let new_header_ptr = *to_alloc as *mut ObjectHeader;
        let new_data_ptr = (*to_alloc).add(HEADER_SIZE);

        // Copy header and data
        ptr::copy_nonoverlapping(header as *const u8, *to_alloc, total_size);

        // Set up new header
        (*new_header_ptr).forwarding = ptr::null_mut();

        // Set forwarding pointer in old location
        (*header).forwarding = new_data_ptr;

        // Advance to-space allocation pointer
        *to_alloc = to_alloc.add(total_size);

        self.move_count.fetch_add(1, Ordering::Relaxed);

        println!("[GC] Copied object {:p} -> {:p} ({} bytes)", ptr, new_data_ptr, size);

        new_data_ptr
    }

    /// Perform a garbage collection
    /// `roots` is a slice of pointers to GC root slots (stack locations containing GC pointers)
    pub unsafe fn collect(&mut self, roots: &mut [*mut *mut u8]) {
        println!("\n[GC] ===== Starting GC #{} =====",
            self.gc_count.load(Ordering::Relaxed) + 1);
        println!("[GC] {} roots to scan", roots.len());

        // Reset to-space allocation pointer
        let mut to_alloc = self.to_space;

        // Phase 1: Copy all root objects
        for root in roots.iter() {
            let old_ptr = **root;
            if !old_ptr.is_null() && self.is_gc_ptr(old_ptr) {
                let new_ptr = self.copy_object(old_ptr, &mut to_alloc);
                // Update the root to point to new location!
                **root = new_ptr;
                println!("[GC] Updated root {:p}: {:p} -> {:p}", *root, old_ptr, new_ptr);
            }
        }

        // Phase 2: Scan copied objects for more references (Cheney's algorithm)
        // For this simple demo, we assume objects don't contain pointers
        // A real implementation would scan object contents here

        // Phase 3: Swap spaces
        std::mem::swap(&mut self.from_space, &mut self.to_space);
        self.alloc_ptr.store(to_alloc as usize, Ordering::Relaxed);
        self.from_space_end = self.from_space as usize + SEMI_SPACE_SIZE;

        // Clear the old from-space (now to-space)
        ptr::write_bytes(self.to_space, 0xDD, SEMI_SPACE_SIZE);

        self.gc_count.fetch_add(1, Ordering::Relaxed);

        let used = to_alloc as usize - self.from_space as usize;
        println!("[GC] ===== GC complete: {} bytes live, {} objects moved =====\n",
            used, self.move_count.load(Ordering::Relaxed));
    }
}

impl Drop for GcState {
    fn drop(&mut self) {
        unsafe {
            let layout = Layout::from_size_align(SEMI_SPACE_SIZE, 4096).unwrap();
            dealloc(self.space_a, layout);
            dealloc(self.space_b, layout);
        }
    }
}

// ============================================================================
// Global GC state and C-compatible API
// ============================================================================

pub static mut GC: Option<GcState> = None;

/// Initialize the GC (must be called before any allocations)
#[no_mangle]
pub extern "C" fn gc_init() {
    unsafe {
        GC = Some(GcState::new());
    }
}

/// Allocate a GC-managed object
#[no_mangle]
pub extern "C" fn gc_alloc(size: u64) -> *mut u8 {
    unsafe {
        match &GC {
            Some(gc) => gc.alloc(size as usize),
            None => {
                gc_init();
                GC.as_ref().unwrap().alloc(size as usize)
            }
        }
    }
}

/// Use an object (prevents optimization from removing it)
#[no_mangle]
pub extern "C" fn use_object(ptr: *mut u8) {
    if ptr.is_null() {
        println!("[USE] null pointer");
    } else {
        // Read the first 8 bytes of the object
        let value = unsafe { *(ptr as *const u64) };
        println!("[USE] object at {:p}, first word = {:#x}", ptr, value);
    }
}

/// Trigger a GC with the given roots
/// This is called from our test harness, not from LLVM-generated code
#[no_mangle]
pub extern "C" fn gc_collect_with_roots(roots: *mut *mut u8, num_roots: usize) {
    unsafe {
        if let Some(gc) = &mut GC {
            let roots_slice = std::slice::from_raw_parts_mut(roots as *mut *mut *mut u8, num_roots);
            gc.collect(roots_slice);
        }
    }
}

/// Get GC statistics
#[no_mangle]
pub extern "C" fn gc_stats() {
    unsafe {
        if let Some(gc) = &GC {
            println!("[GC STATS] Allocations: {}, GCs: {}, Objects moved: {}",
                gc.alloc_count.load(Ordering::Relaxed),
                gc.gc_count.load(Ordering::Relaxed),
                gc.move_count.load(Ordering::Relaxed));
        }
    }
}

/// Force GC on next allocation (for testing)
#[no_mangle]
pub extern "C" fn gc_force_next() {
    unsafe {
        if let Some(gc) = &GC {
            gc.force_gc.store(true, Ordering::Relaxed);
        }
    }
}
