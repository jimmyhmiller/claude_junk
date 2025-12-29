//! Shadow Stack for GC Root Management
//!
//! This module implements a shadow stack that tracks GC roots for a dynamic
//! language with tagged pointers. The shadow stack is a linked list of frames,
//! where each frame contains slots for tagged values that might be heap pointers.
//!
//! The key insight is that LLVM can't track tagged pointers (i64 values that
//! might or might not be heap pointers) through its statepoint infrastructure.
//! Instead, we maintain an explicit shadow stack that the GC walks at collection
//! time, filtering values by their tag bits.
//!
//! ## Architecture
//!
//! ```text
//! Thread-local state:
//!   shadow_stack_top ──────────────────────────────┐
//!                                                  ▼
//!   ┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
//!   │ Frame (oldest)  │◄───│ Frame           │◄───│ Frame (newest)  │
//!   │ prev: null      │    │ prev: ────────┘ │    │ prev: ────────┘ │
//!   │ num_slots: 2    │    │ num_slots: 3    │    │ num_slots: 1    │
//!   │ slots: [v1, v2] │    │ slots: [a,b,c]  │    │ slots: [x]      │
//!   └─────────────────┘    └─────────────────┘    └─────────────────┘
//! ```
//!
//! ## Usage from generated code
//!
//! ```llvm
//! define i64 @my_function() {
//!   ; Allocate frame with 2 slots on shadow stack
//!   %frame = call ptr @gc_shadow_stack_push(i32 2)
//!
//!   ; First allocation
//!   %obj1 = call i64 @rt_cons(i64 %a, i64 %b)
//!
//!   ; Save to shadow stack slot 0 before next allocation
//!   call void @gc_shadow_stack_store(ptr %frame, i32 0, i64 %obj1)
//!
//!   ; Second allocation (might trigger GC!)
//!   %obj2 = call i64 @rt_cons(i64 %c, i64 %d)
//!
//!   ; Reload from shadow stack (might have been relocated)
//!   %obj1_relocated = call i64 @gc_shadow_stack_load(ptr %frame, i32 0)
//!
//!   ; Pop frame before returning
//!   call void @gc_shadow_stack_pop(ptr %frame)
//!   ret i64 %result
//! }
//! ```

use std::ptr;
use crate::tagged_value::{TaggedValue, is_heap_ptr};

/// Maximum number of slots per frame (for fixed-size frame optimization)
pub const MAX_FRAME_SLOTS: usize = 32;

/// A shadow stack frame
///
/// This is allocated on the native stack, so it's very fast.
/// The frame contains a fixed array of slots to avoid dynamic allocation.
#[repr(C)]
pub struct ShadowFrame {
    /// Pointer to the previous frame (linked list)
    pub prev: *mut ShadowFrame,
    /// Number of slots in use
    pub num_slots: u32,
    /// The slots containing tagged values (potential GC roots)
    pub slots: [TaggedValue; MAX_FRAME_SLOTS],
}

impl ShadowFrame {
    /// Create a new frame with the given number of slots
    #[inline]
    pub fn new(num_slots: u32) -> Self {
        debug_assert!(num_slots as usize <= MAX_FRAME_SLOTS);
        ShadowFrame {
            prev: ptr::null_mut(),
            num_slots,
            slots: [0; MAX_FRAME_SLOTS], // Initialize all slots to 0 (not a valid heap ptr)
        }
    }

    /// Get a slot value
    #[inline]
    pub fn get(&self, index: u32) -> TaggedValue {
        debug_assert!((index as usize) < MAX_FRAME_SLOTS);
        self.slots[index as usize]
    }

    /// Set a slot value
    #[inline]
    pub fn set(&mut self, index: u32, value: TaggedValue) {
        debug_assert!((index as usize) < MAX_FRAME_SLOTS);
        self.slots[index as usize] = value;
    }
}

/// Thread-local shadow stack state
pub struct ShadowStack {
    /// Top of the shadow stack (most recent frame)
    top: *mut ShadowFrame,
    /// Statistics
    pub frames_pushed: usize,
    pub max_depth: usize,
    current_depth: usize,
}

impl ShadowStack {
    pub const fn new() -> Self {
        ShadowStack {
            top: ptr::null_mut(),
            frames_pushed: 0,
            max_depth: 0,
            current_depth: 0,
        }
    }

    /// Push a frame onto the shadow stack
    #[inline]
    pub fn push(&mut self, frame: *mut ShadowFrame) {
        unsafe {
            (*frame).prev = self.top;
            self.top = frame;
        }
        self.frames_pushed += 1;
        self.current_depth += 1;
        if self.current_depth > self.max_depth {
            self.max_depth = self.current_depth;
        }
    }

    /// Pop a frame from the shadow stack
    #[inline]
    pub fn pop(&mut self, frame: *mut ShadowFrame) {
        debug_assert_eq!(self.top, frame, "Shadow stack frame mismatch!");
        unsafe {
            self.top = (*frame).prev;
        }
        self.current_depth -= 1;
    }

    /// Get the top frame
    #[inline]
    pub fn top(&self) -> *mut ShadowFrame {
        self.top
    }

    /// Iterate over all roots in the shadow stack
    /// Calls the callback for each slot that contains a heap pointer
    pub fn for_each_root<F>(&self, mut callback: F)
    where
        F: FnMut(*mut TaggedValue),
    {
        let mut frame = self.top;
        while !frame.is_null() {
            unsafe {
                let f = &mut *frame;
                for i in 0..f.num_slots as usize {
                    let slot_ptr = &mut f.slots[i] as *mut TaggedValue;
                    // Only report slots that contain heap pointers
                    if is_heap_ptr(*slot_ptr) {
                        callback(slot_ptr);
                    }
                }
                frame = f.prev;
            }
        }
    }

    /// Count total roots (for debugging)
    pub fn count_roots(&self) -> usize {
        let mut count = 0;
        self.for_each_root(|_| count += 1);
        count
    }

    /// Print the shadow stack (for debugging)
    pub fn dump(&self) {
        println!("[Shadow Stack] Dumping stack (top to bottom):");
        let mut frame = self.top;
        let mut depth = 0;
        while !frame.is_null() {
            unsafe {
                let f = &*frame;
                println!("  Frame {} @ {:p}: {} slots", depth, frame, f.num_slots);
                for i in 0..f.num_slots as usize {
                    let val = f.slots[i];
                    let is_ptr = is_heap_ptr(val);
                    println!("    [{:2}]: {:#018x} {}", i, val,
                        if is_ptr { "<- HEAP PTR" } else { "" });
                }
                frame = f.prev;
                depth += 1;
            }
        }
        println!("[Shadow Stack] Total depth: {}", depth);
    }
}

// ============================================================================
// Global shadow stack (thread-local in production, global for simplicity here)
// ============================================================================

pub static mut SHADOW_STACK: ShadowStack = ShadowStack::new();

/// Get the shadow stack
#[inline]
pub fn shadow_stack() -> &'static mut ShadowStack {
    unsafe { &mut SHADOW_STACK }
}

// ============================================================================
// C-compatible API for use from generated code
// ============================================================================

/// Push a new frame with `num_slots` slots onto the shadow stack.
/// Returns a pointer to the frame (which is allocated by the caller on their stack).
///
/// In practice, the generated code allocates the frame on its stack:
/// ```llvm
/// %frame = alloca %ShadowFrame
/// call void @gc_shadow_stack_push(ptr %frame, i32 2)
/// ```
#[no_mangle]
pub extern "C" fn gc_shadow_stack_push(frame: *mut ShadowFrame, num_slots: u32) {
    unsafe {
        (*frame).prev = ptr::null_mut();
        (*frame).num_slots = num_slots;
        // Zero the slots
        for i in 0..num_slots as usize {
            (*frame).slots[i] = 0;
        }
        shadow_stack().push(frame);
    }
}

/// Pop a frame from the shadow stack
#[no_mangle]
pub extern "C" fn gc_shadow_stack_pop(frame: *mut ShadowFrame) {
    shadow_stack().pop(frame);
}

/// Store a value into a shadow stack slot
#[no_mangle]
pub extern "C" fn gc_shadow_stack_store(frame: *mut ShadowFrame, index: u32, value: TaggedValue) {
    unsafe {
        (*frame).slots[index as usize] = value;
    }
}

/// Load a value from a shadow stack slot
#[no_mangle]
pub extern "C" fn gc_shadow_stack_load(frame: *mut ShadowFrame, index: u32) -> TaggedValue {
    unsafe {
        (*frame).slots[index as usize]
    }
}

// ============================================================================
// Rust-side helpers for ergonomic use
// ============================================================================

/// A guard that automatically pops the shadow stack frame when dropped
pub struct FrameGuard {
    frame: *mut ShadowFrame,
}

impl Drop for FrameGuard {
    fn drop(&mut self) {
        shadow_stack().pop(self.frame);
    }
}

/// Create a shadow stack frame scope
///
/// Usage:
/// ```rust
/// let mut frame = ShadowFrame::new(2);
/// let _guard = with_frame(&mut frame);
///
/// frame.set(0, some_value);
/// // ... allocations that might trigger GC ...
/// let relocated = frame.get(0);
///
/// // frame is automatically popped when _guard goes out of scope
/// ```
pub fn with_frame(frame: &mut ShadowFrame) -> FrameGuard {
    shadow_stack().push(frame as *mut ShadowFrame);
    FrameGuard { frame: frame as *mut ShadowFrame }
}

/// Macro for convenient frame management
///
/// Usage:
/// ```rust
/// with_roots!(frame, 2, {
///     frame.set(0, value1);
///     frame.set(1, value2);
///
///     // ... do allocations ...
///
///     let v1 = frame.get(0);
///     let v2 = frame.get(1);
/// });
/// ```
#[macro_export]
macro_rules! with_roots {
    ($frame:ident, $num_slots:expr, $body:block) => {{
        let mut $frame = $crate::shadow_stack::ShadowFrame::new($num_slots);
        let _guard = $crate::shadow_stack::with_frame(&mut $frame);
        $body
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tagged_value::{make_fixnum, NIL};

    #[test]
    fn test_shadow_frame() {
        let mut frame = ShadowFrame::new(3);
        frame.set(0, make_fixnum(42));
        frame.set(1, NIL);
        frame.set(2, make_fixnum(-1));

        assert_eq!(frame.get(0), make_fixnum(42));
        assert_eq!(frame.get(1), NIL);
        assert_eq!(frame.get(2), make_fixnum(-1));
    }

    #[test]
    fn test_shadow_stack_push_pop() {
        let mut frame1 = ShadowFrame::new(1);
        let mut frame2 = ShadowFrame::new(2);

        let stack = shadow_stack();

        stack.push(&mut frame1);
        assert_eq!(stack.top(), &mut frame1 as *mut _);

        stack.push(&mut frame2);
        assert_eq!(stack.top(), &mut frame2 as *mut _);

        stack.pop(&mut frame2);
        assert_eq!(stack.top(), &mut frame1 as *mut _);

        stack.pop(&mut frame1);
        assert!(stack.top().is_null());
    }
}
