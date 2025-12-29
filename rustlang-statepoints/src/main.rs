//! LLVM Statepoints Proof of Concept
//!
//! This project demonstrates using LLVM's native statepoint infrastructure
//! for garbage collection, WITHOUT a runtime shadow stack.
//!
//! Key approach:
//! - Use `ptr addrspace(1)` for GC-tracked pointers in LLVM IR
//! - Let LLVM's RewriteStatepointsForGC pass track liveness
//! - Use MCJIT memory manager to capture stackmaps during JIT
//! - Use stackmaps to find GC roots at runtime

mod tagged_value;
mod stackmap;
mod gc_runtime;
mod jit_runner;
mod jit_mcjit;

fn main() {
    jit_mcjit::run_demo();
}
