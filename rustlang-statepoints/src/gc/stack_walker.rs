//! Stack walker for GC root scanning
//!
//! Walks the frame pointer chain to find GC roots using stack maps.

use crate::types::BuiltInTypes;
use super::StackMap;

/// Stack walker abstraction for finding heap pointers
pub struct StackWalker;

impl StackWalker {
    /// Get the live portion of the stack as a slice
    #[allow(dead_code)]
    pub fn get_live_stack(stack_base: usize, frame_pointer: usize) -> &'static [usize] {
        let distance_till_end = stack_base - frame_pointer;
        let num_words = (distance_till_end / 8) + 1;
        unsafe { std::slice::from_raw_parts(frame_pointer as *const usize, num_words) }
    }

    /// Collect all heap pointers from the stack with explicit return address
    pub fn collect_stack_roots_with_return_addr(
        stack_base: usize,
        frame_pointer: usize,
        gc_return_addr: usize,
        stack_map: &StackMap,
    ) -> Vec<(usize, usize)> {
        let mut roots = Vec::with_capacity(32);
        Self::walk_stack_roots_with_return_addr(
            stack_base,
            frame_pointer,
            gc_return_addr,
            stack_map,
            |addr, pointer| {
                roots.push((addr, pointer));
            },
        );
        roots
    }

    /// Walk the stack using the frame pointer chain with explicit return address
    ///
    /// Key insight: The return address at [FP+8] describes the CALLER's frame, not the current frame.
    /// So we track the "pending" return address from the previous frame to know how to scan the current frame.
    pub fn walk_stack_roots_with_return_addr<F>(
        stack_base: usize,
        frame_pointer: usize,
        gc_return_addr: usize,
        stack_map: &StackMap,
        mut callback: F,
    ) where
        F: FnMut(usize, usize),
    {
        let mut fp = frame_pointer;
        let mut pending_return_addr = gc_return_addr;

        #[cfg(feature = "debug-gc")]
        eprintln!(
            "[GC DEBUG] walk_stack_roots_with_return_addr: stack_base={:#x}, frame_pointer={:#x}, gc_return_addr={:#x}",
            stack_base, frame_pointer, gc_return_addr
        );

        while fp != 0 && fp < stack_base {
            let caller_fp = unsafe { *(fp as *const usize) };
            let return_addr_for_caller = unsafe { *((fp + 8) as *const usize) };

            #[cfg(feature = "debug-gc")]
            eprintln!(
                "[GC DEBUG] Frame at FP={:#x}, pending_return_addr={:#x}, caller_fp={:#x}",
                fp, pending_return_addr, caller_fp
            );

            // Use pending_return_addr to scan the CURRENT frame (fp)
            if pending_return_addr != 0 {
                if let Some(details) = stack_map.find_stack_data(pending_return_addr) {
                    #[cfg(feature = "debug-gc")]
                    eprintln!(
                        "[GC DEBUG] Scanning frame at FP={:#x}: fn={:?}, locals={}, max_stack={}, cur_stack={}",
                        fp,
                        details.function_name,
                        details.number_of_locals,
                        details.max_stack_size,
                        details.current_stack_size
                    );

                    let active_slots = details.number_of_locals + details.current_stack_size;

                    for i in 0..active_slots {
                        let slot_addr = fp - 8 - (i * 8);
                        let slot_value = unsafe { *(slot_addr as *const usize) };

                        if BuiltInTypes::is_heap_pointer(slot_value) {
                            let untagged = BuiltInTypes::untag(slot_value);
                            if untagged % 8 != 0 {
                                #[cfg(feature = "debug-gc")]
                                eprintln!(
                                    "[GC DEBUG] SKIPPING unaligned pointer: slot[{}] @ {:#x} = {:#x}",
                                    i, slot_addr, slot_value
                                );
                                continue;
                            }
                            callback(slot_addr, slot_value);
                        }
                    }
                }
            }

            if caller_fp != 0 && caller_fp <= fp {
                #[cfg(feature = "debug-gc")]
                eprintln!(
                    "[GC DEBUG] FP chain invalid: caller_fp={:#x} <= fp={:#x}, stopping",
                    caller_fp, fp
                );
                break;
            }

            fp = caller_fp;
            pending_return_addr = return_addr_for_caller;
        }

        #[cfg(feature = "debug-gc")]
        eprintln!("[GC DEBUG] walk_stack_roots_with_return_addr done");
    }

    /// Get a mutable slice of the live stack for updating pointers after GC
    #[allow(dead_code)]
    pub fn get_live_stack_mut(stack_base: usize, frame_pointer: usize) -> &'static mut [usize] {
        let distance_till_end = stack_base - frame_pointer;
        let num_words = (distance_till_end / 8) + 1;
        unsafe { std::slice::from_raw_parts_mut(frame_pointer as *mut usize, num_words) }
    }
}
