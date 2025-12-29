//! Proper single-process JIT with MCJIT memory manager for stackmap capture
//!
//! This uses LLVM's execution engine correctly:
//! 1. Generate IR with inkwell
//! 2. Run RewriteStatepointsForGC pass
//! 3. Create MCJIT with custom memory manager
//! 4. Memory manager captures __llvm_stackmaps section during JIT
//! 5. Execute with correct stackmaps from the SAME compilation
//!
//! No double compilation, no external processes!

use inkwell::context::Context;
use inkwell::execution_engine::{ExecutionEngine, JitFunction, MCJITMemoryManager, MCJITMemoryManagerCallbacks};
use inkwell::module::Linkage;
use inkwell::passes::PassBuilderOptions;
use inkwell::targets::{InitializationConfig, Target, TargetMachine, CodeModel, RelocMode};
use inkwell::OptimizationLevel;
use inkwell::AddressSpace;
use std::ffi::CStr;
use std::sync::Mutex;

use crate::jit_runner;
use crate::stackmap::StackMap;
use crate::tagged_value::*;

/// GC address space (addrspace 1)
fn gc_address_space() -> AddressSpace {
    AddressSpace::from(1)
}

type BuildListFn = unsafe extern "C" fn() -> u64;

/// State captured by the memory manager callbacks
struct MemoryManagerState {
    /// Address and size of the stackmaps section (if found)
    stackmaps: Option<(usize, usize)>,  // (address as usize, size)
    /// All allocated code sections
    code_sections: Vec<(usize, usize)>,  // (address as usize, size)
    /// All allocated data sections
    data_sections: Vec<(usize, usize)>,  // (address as usize, size)
}

// Global state for the memory manager (required because callbacks are C functions)
static MM_STATE: Mutex<MemoryManagerState> = Mutex::new(MemoryManagerState {
    stackmaps: None,
    code_sections: Vec::new(),
    data_sections: Vec::new(),
});

extern "C" fn allocate_code_section(
    _opaque: *mut libc::c_void,
    size: libc::uintptr_t,
    alignment: libc::c_uint,
    _section_id: libc::c_uint,
    section_name: *const libc::c_char,
) -> *mut u8 {
    let name = unsafe { CStr::from_ptr(section_name).to_string_lossy() };
    println!("  [MM] Allocating code section: {} ({} bytes, align {})", name, size, alignment);

    // Allocate with mmap for executable memory
    let ptr = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
            -1,
            0,
        ) as *mut u8
    };

    if ptr.is_null() || ptr == libc::MAP_FAILED as *mut u8 {
        panic!("mmap failed for code section");
    }

    if let Ok(mut state) = MM_STATE.lock() {
        state.code_sections.push((ptr as usize, size));
    }

    ptr
}

extern "C" fn allocate_data_section(
    _opaque: *mut libc::c_void,
    size: libc::uintptr_t,
    alignment: libc::c_uint,
    _section_id: libc::c_uint,
    section_name: *const libc::c_char,
    _is_read_only: i32,
) -> *mut u8 {
    let name = unsafe { CStr::from_ptr(section_name).to_string_lossy() };
    println!("  [MM] Allocating data section: {} ({} bytes, align {})", name, size, alignment);

    // Allocate memory
    let ptr = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
            -1,
            0,
        ) as *mut u8
    };

    if ptr.is_null() || ptr == libc::MAP_FAILED as *mut u8 {
        panic!("mmap failed for data section");
    }

    // Check if this is the stackmaps section
    if name == "__llvm_stackmaps" || name == ".llvm_stackmaps" {
        println!("  [MM] *** Found stackmaps section at {:p} ***", ptr);
        if let Ok(mut state) = MM_STATE.lock() {
            state.stackmaps = Some((ptr as usize, size));
        }
    }

    if let Ok(mut state) = MM_STATE.lock() {
        state.data_sections.push((ptr as usize, size));
    }

    ptr
}

extern "C" fn finalize_memory(
    _opaque: *mut libc::c_void,
    _err_msg: *mut *mut libc::c_char,
) -> i32 {
    println!("  [MM] Finalizing memory...");

    if let Ok(state) = MM_STATE.lock() {
        // Make code sections executable
        for (addr, size) in &state.code_sections {
            unsafe {
                if libc::mprotect(*addr as *mut libc::c_void, *size, libc::PROT_READ | libc::PROT_EXEC) != 0 {
                    eprintln!("  [MM] Warning: mprotect failed for code section");
                }
            }
        }
    }

    0 // Success
}

extern "C" fn destroy(_opaque: *mut libc::c_void) {
    println!("  [MM] Memory manager destroyed");
    // Note: We don't free the memory here because the execution engine might still need it
}

pub fn run_demo() {
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║   MCJIT + Memory Manager - True Single-Compile JIT           ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    // Reset state
    if let Ok(mut state) = MM_STATE.lock() {
        state.stackmaps = None;
        state.code_sections.clear();
        state.data_sections.clear();
    }

    Target::initialize_native(&InitializationConfig::default())
        .expect("Failed to initialize native target");

    jit_runner::init();
    jit_runner::set_verbose(true);

    let (cons_addr, gc_addr) = jit_runner::get_runtime_symbols();
    println!("Runtime functions:");
    println!("  rt_cons_raw_jit: {:#x}", cons_addr);
    println!("  rt_gc_jit: {:#x}\n", gc_addr);

    // Create context and module
    let context = Context::create();
    let module = context.create_module("jit_mcjit");
    let builder = context.create_builder();

    // Types
    let i64_type = context.i64_type();
    let void_type = context.void_type();
    let gc_ptr_type = context.ptr_type(gc_address_space());

    // Declare external runtime functions
    let cons_fn = module.add_function(
        "rt_cons_raw_jit",
        gc_ptr_type.fn_type(&[i64_type.into(), i64_type.into()], false),
        Some(Linkage::External),
    );

    let gc_fn = module.add_function(
        "rt_gc_jit",
        void_type.fn_type(&[], false),
        Some(Linkage::External),
    );

    // Create build_list function
    println!("═══ Generating IR ═══\n");

    let fn_type = i64_type.fn_type(&[], false);
    let function = module.add_function("build_list", fn_type, None);
    function.set_gc("statepoint-example");

    let entry = context.append_basic_block(function, "entry");
    builder.position_at_end(entry);

    // Constants
    let nil = i64_type.const_int(NIL, false);
    let one = i64_type.const_int(make_fixnum(1), false);
    let two = i64_type.const_int(make_fixnum(2), false);
    let three = i64_type.const_int(make_fixnum(3), false);

    // cell3 = cons(3, nil)
    let cell3 = builder
        .build_call(cons_fn, &[three.into(), nil.into()], "cell3")
        .unwrap()
        .try_as_basic_value()
        .left()
        .unwrap()
        .into_pointer_value();

    // Trigger GC
    builder.build_call(gc_fn, &[], "gc1").unwrap();

    // cell2 = cons(2, cell3)
    let cell3_i64 = builder.build_ptr_to_int(cell3, i64_type, "cell3_i64").unwrap();
    let cell2 = builder
        .build_call(cons_fn, &[two.into(), cell3_i64.into()], "cell2")
        .unwrap()
        .try_as_basic_value()
        .left()
        .unwrap()
        .into_pointer_value();

    // Trigger GC
    builder.build_call(gc_fn, &[], "gc2").unwrap();

    // cell1 = cons(1, cell2)
    let cell2_i64 = builder.build_ptr_to_int(cell2, i64_type, "cell2_i64").unwrap();
    let cell1 = builder
        .build_call(cons_fn, &[one.into(), cell2_i64.into()], "cell1")
        .unwrap()
        .try_as_basic_value()
        .left()
        .unwrap()
        .into_pointer_value();

    let result = builder.build_ptr_to_int(cell1, i64_type, "result").unwrap();
    builder.build_return(Some(&result)).unwrap();

    println!("Generated IR (before statepoints)\n");

    // Run the RewriteStatepointsForGC pass
    println!("═══ Running RewriteStatepointsForGC Pass ═══\n");

    let target_triple = TargetMachine::get_default_triple();
    let target = Target::from_triple(&target_triple).unwrap();
    let target_machine = target
        .create_target_machine(
            &target_triple,
            "generic",
            "",
            OptimizationLevel::None,
            RelocMode::PIC,
            CodeModel::Default,
        )
        .unwrap();

    module
        .run_passes("rewrite-statepoints-for-gc", &target_machine, PassBuilderOptions::create())
        .expect("Failed to run statepoint pass");

    println!("Statepoints inserted\n");

    // Create memory manager with our callbacks
    println!("═══ Creating MCJIT with Custom Memory Manager ═══\n");

    let callbacks = MCJITMemoryManagerCallbacks {
        allocate_code_section,
        allocate_data_section,
        finalize_memory,
        destroy: Some(destroy),
    };

    let memory_manager = unsafe {
        MCJITMemoryManager::new(std::ptr::null_mut(), callbacks)
            .expect("Failed to create memory manager")
    };

    // Create execution engine with our memory manager
    let execution_engine = unsafe {
        ExecutionEngine::create_mcjit_with_memory_manager(&module, memory_manager, None)
            .expect("Failed to create execution engine")
    };

    // Add global mappings for runtime functions
    execution_engine.add_global_mapping(&cons_fn, cons_addr);
    execution_engine.add_global_mapping(&gc_fn, gc_addr);

    // Get the JIT'd function
    let build_list: JitFunction<BuildListFn> = unsafe {
        execution_engine
            .get_function("build_list")
            .expect("Failed to get build_list function")
    };

    let fn_addr = unsafe { build_list.as_raw() as usize };
    println!("\nbuild_list JIT'd at {:#x}", fn_addr);

    // Extract and load stackmaps from the SAME compilation
    println!("\n═══ Loading Stackmaps ═══\n");

    let stackmaps = if let Ok(state) = MM_STATE.lock() {
        state.stackmaps
    } else {
        None
    };

    if let Some((stackmap_addr, stackmap_size)) = stackmaps {
        println!("Stackmaps at {:#x}, {} bytes", stackmap_addr, stackmap_size);

        let stackmap_data = unsafe {
            std::slice::from_raw_parts(stackmap_addr as *const u8, stackmap_size)
        };

        match StackMap::parse(stackmap_data) {
            Ok(sm) => {
                println!("Parsed {} stackmap records:", sm.records.len());
                for func in &sm.functions {
                    println!("  Function: stack_size={}", func.stack_size);
                }
                for (i, record) in sm.records.iter().enumerate() {
                    let gc_locs = sm.get_gc_locations(record);
                    if !gc_locs.is_empty() {
                        println!("  Record {}: offset={:#x}, {} GC ptrs", i, record.instruction_offset, gc_locs.len());
                    }
                }

                // Load into GC runtime
                jit_runner::load_stackmaps(sm, fn_addr);
            }
            Err(e) => {
                println!("Failed to parse stackmaps: {:?}", e);
            }
        }
    } else {
        println!("No stackmaps section found!");
    }

    // Execute
    println!("\n═══ Executing ═══\n");

    let result = unsafe { build_list.call() };

    println!("\n─── Results ───\n");
    println!("Returned value: {:#x}", result);

    if is_heap_ptr(result) {
        println!("List: {}", TaggedValueDisplay(result));

        let v1 = fixnum_value(car(result));
        let v2 = fixnum_value(car(cdr(result)));
        let v3 = fixnum_value(car(cdr(cdr(result))));

        println!("Elements: {}, {}, {}", v1, v2, v3);

        if v1 == 1 && v2 == 2 && v3 == 3 {
            println!("\n✓ SUCCESS: True single-compile JIT with MCJIT memory manager!");
            println!("  - Compiled once with MCJIT");
            println!("  - Captured stackmaps via memory manager callback");
            println!("  - Same code, same offsets, correct GC!");
        }
    }

    jit_runner::stats();
}
