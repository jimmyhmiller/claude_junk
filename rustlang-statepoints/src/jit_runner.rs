//! JIT Runner with Stackmap-based GC
//!
//! This module demonstrates end-to-end execution of JIT-compiled code
//! with GC that uses LLVM stackmaps to find roots on the stack.
//!
//! The flow:
//! 1. Compile statepoint-lowered IR to a shared library
//! 2. Parse stackmaps from the library
//! 3. Load and execute the library
//! 4. When rt_gc is called, walk the stack and use stackmaps to find roots

use std::collections::HashMap;
use std::sync::Mutex;

use crate::gc_runtime::GarbageCollector;
use crate::stackmap::{StackMap, StackMapRecord, Location, LocationType};
use crate::tagged_value::*;

/// Wrapper to make GarbageCollector Send
/// Safety: We only access the GC from a single thread in practice,
/// and we use a Mutex to ensure exclusive access.
struct SendGc(GarbageCollector);
unsafe impl Send for SendGc {}

/// Global state for the JIT runtime
/// This holds the stackmap info and GC, accessible from rt_gc
static JIT_STATE: Mutex<Option<JitState>> = Mutex::new(None);

pub struct JitState {
    /// The garbage collector (wrapped for Send)
    gc: SendGc,
    /// Parsed stackmaps
    pub stackmap: Option<StackMap>,
    /// Map from return address to stackmap record index
    /// Key is (offset from code base)
    pub safepoint_map: HashMap<u32, usize>,
    /// Base address of loaded code
    pub code_base: usize,
    /// Verbose output
    pub verbose: bool,
}

impl JitState {
    pub fn new() -> Self {
        JitState {
            gc: SendGc(GarbageCollector::new()),
            stackmap: None,
            safepoint_map: HashMap::new(),
            code_base: 0,
            verbose: false,
        }
    }
}

/// Initialize the JIT runtime
pub fn init() {
    let mut state = JIT_STATE.lock().unwrap();
    *state = Some(JitState::new());
}

/// Set verbose mode
pub fn set_verbose(v: bool) {
    let mut state = JIT_STATE.lock().unwrap();
    if let Some(ref mut s) = *state {
        s.verbose = v;
        s.gc.0.set_verbose(v);
    }
}

/// Load stackmaps and set code base address
pub fn load_stackmaps(stackmap: StackMap, code_base: usize) {
    let mut state = JIT_STATE.lock().unwrap();
    if let Some(ref mut s) = *state {
        // Build safepoint map: instruction_offset -> record_index
        for (idx, record) in stackmap.records.iter().enumerate() {
            s.safepoint_map.insert(record.instruction_offset, idx);
            if s.verbose {
                println!("  [JIT] Safepoint at offset {:#x} -> record {}",
                         record.instruction_offset, idx);
            }
        }
        s.stackmap = Some(stackmap);
        s.code_base = code_base;

        if s.verbose {
            println!("  [JIT] Loaded stackmaps, code_base={:#x}", code_base);
        }
    }
}

/// Runtime: Allocate a cons cell (called from JIT code)
#[no_mangle]
#[inline(never)]
pub extern "C" fn rt_cons_raw_jit(car: TaggedValue, cdr: TaggedValue) -> *mut u8 {
    let mut state = JIT_STATE.lock().unwrap();
    let s = state.as_mut().expect("JIT not initialized");

    let result = s.gc.0.alloc_cons(car, cdr);
    result as *mut u8
}

/// Runtime: Trigger GC with stack walking (called from JIT code)
///
/// This is the key function that demonstrates stackmap-based root finding.
/// When called from JIT code:
/// 1. Walk the stack to find return addresses
/// 2. For each return address, look up the stackmap
/// 3. Use the stackmap to find live GC pointers
/// 4. Pass them to the GC for collection
#[no_mangle]
#[inline(never)]
pub extern "C" fn rt_gc_jit() {
    let mut state = JIT_STATE.lock().unwrap();
    let s = state.as_mut().expect("JIT not initialized");

    if s.verbose {
        println!("\n  [JIT-GC] rt_gc_jit called - walking stack");
    }

    // Get current frame pointer and stack pointer
    let mut roots: Vec<*mut TaggedValue> = Vec::new();

    unsafe {
        // Get current frame pointer and stack pointer (arm64)
        let fp: *const u8;
        let sp: *const u8;
        std::arch::asm!(
            "mov {fp}, x29",
            "mov {sp}, sp",
            fp = out(reg) fp,
            sp = out(reg) sp,
            options(nomem, nostack)
        );

        if s.verbose {
            println!("  [JIT-GC] Current FP: {:#x}, SP: {:#x}", fp as usize, sp as usize);
        }

        // Walk the stack frames
        // On arm64: [fp] = saved fp, [fp+8] = saved lr (return address)
        let mut current_fp = fp;
        let mut current_sp = sp;
        let mut frame_count = 0;
        const MAX_FRAMES: usize = 20;

        while !current_fp.is_null() && frame_count < MAX_FRAMES {
            // Sanity check: fp should be a reasonable stack address
            if (current_fp as usize) < 0x1000 {
                if s.verbose {
                    println!("  [JIT-GC] Invalid FP {:#x}, stopping walk", current_fp as usize);
                }
                break;
            }

            let saved_fp = *(current_fp as *const *const u8);
            let return_addr = *((current_fp as usize + 8) as *const usize);

            if s.verbose {
                println!("  [JIT-GC] Frame {}: FP={:#x}, RA={:#x}",
                         frame_count, current_fp as usize, return_addr);
            }

            // Check if return address is in our JIT code
            if s.code_base > 0 && return_addr >= s.code_base {
                let offset = (return_addr - s.code_base) as u32;

                // Look for a safepoint near this return address
                // The return address points to the instruction AFTER the call,
                // so we need to search for nearby safepoints
                if let Some(record_idx) = find_safepoint_for_return_addr(s, offset) {
                    if s.verbose {
                        println!("  [JIT-GC] Found safepoint at offset {:#x} -> record {}",
                                offset, record_idx);
                    }

                    // Get the stackmap record
                    let record = &s.stackmap.as_ref().unwrap().records[record_idx];

                    // The JIT code may not use a frame pointer (FP).
                    // It uses SP-relative addressing, and the stackmap tells us
                    // where GC pointers are relative to the JIT code's SP.
                    //
                    // The JIT code's SP at the call site is just above rt_gc_jit's frame.
                    // rt_gc_jit's FP points to its saved FP/LR pair.
                    // JIT's SP = rt_gc_jit's FP + 16 (above the saved FP/LR)
                    //
                    // Actually, this depends on rt_gc_jit's prologue. Let's try
                    // using the saved SP value from rt_gc_jit's frame:
                    // The JIT SP should be at current_fp + 16 (just above our frame)
                    let jit_sp = (current_fp as usize + 16) as *const u8;

                    // For debugging, also show what's at various stack locations
                    if s.verbose {
                        println!("  [JIT-GC] rt_gc_jit FP={:#x}, estimated JIT SP={:#x}",
                                current_fp as usize, jit_sp as usize);

                        // Dump stack around the estimated JIT SP
                        for offset in &[0i64, 8, 16, 24, -8, -16] {
                            let addr = (jit_sp as i64 + offset) as *const u64;
                            let val = *addr;
                            println!("  [JIT-GC] [SP{:+}] = {:#x}", offset, val);
                        }
                    }

                    // Extract roots from this frame using the estimated JIT SP
                    collect_roots_from_frame(
                        s,
                        record,
                        current_fp,
                        jit_sp,
                        &mut roots,
                    );
                }
            }

            // Move to parent frame
            // SP of parent frame is approximately FP + 16 (saved FP + saved LR)
            current_sp = (current_fp as usize + 16) as *const u8;
            current_fp = saved_fp;
            frame_count += 1;

            // Stop at null frame pointer or if we're going backwards
            if saved_fp.is_null() || (saved_fp as usize) < (current_fp as usize) {
                if s.verbose && saved_fp.is_null() {
                    println!("  [JIT-GC] Reached null FP, stopping");
                }
                break;
            }
        }
    }

    if s.verbose {
        println!("  [JIT-GC] Found {} roots from stackmaps", roots.len());
    }

    // Run GC with the collected roots
    if !roots.is_empty() {
        s.gc.0.collect_with_roots(&mut roots);
    } else if s.verbose {
        println!("  [JIT-GC] No roots found, skipping collection");
    }
}

/// Find a safepoint record for a return address
fn find_safepoint_for_return_addr(state: &JitState, offset: u32) -> Option<usize> {
    // Exact match first
    if let Some(&idx) = state.safepoint_map.get(&offset) {
        return Some(idx);
    }

    // Search within a small window (the call instruction varies in size)
    for delta in 0..16u32 {
        if let Some(&idx) = state.safepoint_map.get(&(offset.wrapping_sub(delta))) {
            return Some(idx);
        }
    }

    None
}

/// Collect roots from a stack frame using stackmap info
fn collect_roots_from_frame(
    state: &JitState,
    record: &StackMapRecord,
    fp: *const u8,
    sp: *const u8,
    roots: &mut Vec<*mut TaggedValue>,
) {
    let stackmap = state.stackmap.as_ref().unwrap();
    let gc_locs = stackmap.get_gc_locations(record);

    if state.verbose {
        println!("  [JIT-GC] Record has {} GC location pairs", gc_locs.len());
    }

    for (_base_loc, derived_loc) in gc_locs {
        if state.verbose {
            println!("  [JIT-GC] Processing location: {} (reg={})", derived_loc, derived_loc.reg);
        }

        // Resolve the location to a stack address
        if let Some(addr) = resolve_location_arm64(&derived_loc, fp, sp) {
            if state.verbose {
                println!("  [JIT-GC] Resolved to address {:#x}", addr as usize);
            }

            // Check if this looks like a valid heap pointer
            let value = unsafe { *(addr as *const TaggedValue) };

            if is_heap_ptr(value) {
                if state.verbose {
                    println!("  [JIT-GC] Found root at {} = {:#x}", derived_loc, value);
                }
                roots.push(addr as *mut TaggedValue);
            } else if state.verbose {
                println!("  [JIT-GC] Location {} has non-heap value {:#x}", derived_loc, value);
            }
        } else if state.verbose {
            println!("  [JIT-GC] Could not resolve location {}", derived_loc);
        }
    }
}

/// Resolve a stackmap location to a memory address on arm64
fn resolve_location_arm64(loc: &Location, fp: *const u8, sp: *const u8) -> Option<*mut u8> {
    match loc.ty {
        LocationType::Indirect => {
            // Indirect: value is at [reg + offset]
            // On arm64, DWARF register numbers:
            // x29 (FP) = 29
            // x31 (SP) = 31
            let base = match loc.reg {
                29 => fp, // x29 = FP
                31 => sp, // SP
                // For other registers (x0-x28), we'd need saved context
                // LLVM might spill to SP-relative locations
                _ => {
                    // For now, try SP-relative as a fallback
                    // Many calling conventions use SP-relative addressing
                    sp
                }
            };
            let addr = unsafe { base.offset(loc.offset as isize) };
            Some(addr as *mut u8)
        }
        LocationType::Direct => {
            // Direct: the address itself is reg + offset
            let base = match loc.reg {
                29 => fp,
                31 => sp,
                _ => sp,
            };
            let addr = unsafe { base.offset(loc.offset as isize) };
            Some(addr as *mut u8)
        }
        LocationType::Register => {
            // Value is in a register - can't access without saved context
            None
        }
        _ => None,
    }
}

/// Get function pointers to runtime functions (prevents dead-code elimination)
/// Returns (rt_cons_raw_jit_addr, rt_gc_jit_addr)
pub fn get_runtime_symbols() -> (usize, usize) {
    (
        rt_cons_raw_jit as *const () as usize,
        rt_gc_jit as *const () as usize,
    )
}

/// Helper to print GC stats
pub fn stats() {
    let state = JIT_STATE.lock().unwrap();
    if let Some(ref s) = *state {
        s.gc.0.stats();
    }
}

/// Get the GC for direct manipulation (testing)
pub fn with_gc<F, R>(f: F) -> R
where
    F: FnOnce(&mut GarbageCollector) -> R,
{
    let mut state = JIT_STATE.lock().unwrap();
    let s = state.as_mut().expect("JIT not initialized");
    f(&mut s.gc.0)
}
