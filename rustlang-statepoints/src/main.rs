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
mod mmtk_binding;
#[macro_use]
mod lang;

use tagged_value::*;

fn main() {
    // Check command line args
    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 && args[1] == "--mmtk" {
        run_mmtk_demo();
    } else if args.len() > 1 && args[1] == "--mmtk-jit" {
        run_mmtk_jit_demo();
    } else if args.len() > 1 && args[1] == "--lang" {
        run_lang_demo();
    } else {
        // Default: run original demo with custom GC
        jit_mcjit::run_demo();
    }
}

/// Demo that tests the MMTk integration
fn run_mmtk_demo() {
    println!("═══════════════════════════════════════════════════════════════════");
    println!("  MMTk Integration Demo");
    println!("═══════════════════════════════════════════════════════════════════\n");

    // Initialize logging
    env_logger::init();

    // Initialize MMTk
    println!("1. Initializing MMTk...");
    mmtk_binding::init_mmtk();
    println!("   ✓ MMTk initialized");

    // Bind mutator for the current thread
    println!("\n2. Binding mutator for current thread...");
    mmtk_binding::bind_mutator();
    println!("   ✓ Mutator bound");

    // Enable collection
    println!("\n3. Enabling garbage collection...");
    mmtk_binding::enable_collection();
    println!("   ✓ Collection enabled");

    // Allocate some objects
    println!("\n4. Allocating cons cells via MMTk...");

    let a = mmtk_binding::alloc_cons(make_fixnum(1), NIL);
    println!("   Allocated cons (1 . nil) at {:#x}", a);

    let b = mmtk_binding::alloc_cons(make_fixnum(2), a);
    println!("   Allocated cons (2 . prev) at {:#x}", b);

    let c = mmtk_binding::alloc_cons(make_fixnum(3), b);
    println!("   Allocated cons (3 . prev) at {:#x}", c);

    // Verify the list structure
    println!("\n5. Verifying list structure...");
    let c_cons = c as *const ConsCell;
    unsafe {
        let car = (*c_cons).car;
        let cdr = (*c_cons).cdr;
        println!("   List head car: {} (expected: 3)", fixnum_value(car));
        println!("   List head cdr: {:#x}", cdr);

        if is_heap_ptr(cdr) {
            let b_cons = cdr as *const ConsCell;
            let b_car = (*b_cons).car;
            println!("   Second element car: {} (expected: 2)", fixnum_value(b_car));
        }
    }

    // Allocate more to potentially trigger GC
    println!("\n6. Allocating more objects to test memory pressure...");
    for i in 0..100 {
        let _ = mmtk_binding::alloc_cons(make_fixnum(i), NIL);
    }
    println!("   Allocated 100 more cons cells");

    println!("\n═══════════════════════════════════════════════════════════════════");
    println!("  MMTk Demo Complete!");
    println!("═══════════════════════════════════════════════════════════════════");
}

/// Demo that tests MMTk with JIT-compiled code using LLVM statepoints
fn run_mmtk_jit_demo() {
    use inkwell::context::Context;
    use inkwell::execution_engine::{ExecutionEngine, JitFunction, MCJITMemoryManager, MCJITMemoryManagerCallbacks};
    use inkwell::module::Linkage;
    use inkwell::passes::PassBuilderOptions;
    use inkwell::targets::{InitializationConfig, Target, TargetMachine, CodeModel, RelocMode};
    use inkwell::OptimizationLevel;
    use inkwell::AddressSpace;
    use std::ffi::CStr;
    use std::sync::Mutex;
    use crate::stackmap::StackMap;

    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║   MMTk + LLVM Statepoints JIT Demo                               ║");
    println!("╚══════════════════════════════════════════════════════════════════╝\n");

    // Initialize logging
    env_logger::init();

    // ===== Memory Manager State =====
    struct MemoryManagerState {
        stackmaps: Option<(usize, usize)>,
        code_sections: Vec<(usize, usize)>,
        data_sections: Vec<(usize, usize)>,
    }

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
            for (addr, size) in &state.code_sections {
                unsafe {
                    libc::mprotect(*addr as *mut libc::c_void, *size, libc::PROT_READ | libc::PROT_EXEC);
                }
            }
        }
        0
    }

    extern "C" fn destroy(_opaque: *mut libc::c_void) {
        println!("  [MM] Memory manager destroyed");
    }

    fn gc_address_space() -> AddressSpace {
        AddressSpace::from(1)
    }

    type BuildListFn = unsafe extern "C" fn() -> u64;

    // ===== Initialize MMTk =====
    println!("1. Initializing MMTk...");
    mmtk_binding::init_mmtk();
    println!("   ✓ MMTk initialized");

    println!("\n2. Binding mutator...");
    mmtk_binding::bind_mutator();
    println!("   ✓ Mutator bound");

    println!("\n3. Enabling collection...");
    mmtk_binding::enable_collection();
    println!("   ✓ Collection enabled");

    // Reset memory manager state
    if let Ok(mut state) = MM_STATE.lock() {
        state.stackmaps = None;
        state.code_sections.clear();
        state.data_sections.clear();
    }

    // ===== Initialize LLVM =====
    println!("\n4. Initializing LLVM...");
    Target::initialize_native(&InitializationConfig::default())
        .expect("Failed to initialize native target");
    println!("   ✓ LLVM initialized");

    // Get MMTk runtime function addresses
    let (cons_addr, gc_addr) = mmtk_binding::get_runtime_symbols();
    println!("\n5. Runtime functions:");
    println!("   rt_cons_raw_mmtk: {:#x}", cons_addr);
    println!("   rt_gc_mmtk: {:#x}", gc_addr);

    // ===== Generate IR =====
    println!("\n6. Generating LLVM IR...");
    let context = Context::create();
    let module = context.create_module("mmtk_jit");
    let builder = context.create_builder();

    let i64_type = context.i64_type();
    let void_type = context.void_type();
    let gc_ptr_type = context.ptr_type(gc_address_space());

    // Declare runtime functions (will be resolved by MMTk)
    let cons_fn = module.add_function(
        "rt_cons_raw_mmtk",
        gc_ptr_type.fn_type(&[i64_type.into(), i64_type.into()], false),
        Some(Linkage::External),
    );

    let gc_fn = module.add_function(
        "rt_gc_mmtk",
        void_type.fn_type(&[], false),
        Some(Linkage::External),
    );

    // Create build_list function
    let fn_type = i64_type.fn_type(&[], false);
    let function = module.add_function("build_list", fn_type, None);
    function.set_gc("statepoint-example");

    let entry = context.append_basic_block(function, "entry");
    builder.position_at_end(entry);

    // Build list (3 2 1) with GC points between allocations
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

    // GC safepoint
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

    // GC safepoint
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

    println!("   ✓ IR generated");

    // ===== Run Statepoint Pass =====
    println!("\n7. Running RewriteStatepointsForGC pass...");
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
    println!("   ✓ Statepoints inserted");

    // ===== Create MCJIT with Memory Manager =====
    println!("\n8. Creating MCJIT with custom memory manager...");

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

    let execution_engine = unsafe {
        ExecutionEngine::create_mcjit_with_memory_manager(&module, memory_manager, None)
            .expect("Failed to create execution engine")
    };

    // Map runtime functions
    execution_engine.add_global_mapping(&cons_fn, cons_addr);
    execution_engine.add_global_mapping(&gc_fn, gc_addr);

    let build_list: JitFunction<BuildListFn> = unsafe {
        execution_engine
            .get_function("build_list")
            .expect("Failed to get build_list function")
    };

    let fn_addr = unsafe { build_list.as_raw() as usize };
    println!("   ✓ JIT compiled, function at {:#x}", fn_addr);

    // ===== Load Stackmaps into MMTk =====
    println!("\n9. Loading stackmaps into MMTk...");

    let stackmaps = if let Ok(state) = MM_STATE.lock() {
        state.stackmaps
    } else {
        None
    };

    if let Some((stackmap_addr, stackmap_size)) = stackmaps {
        let stackmap_data = unsafe {
            std::slice::from_raw_parts(stackmap_addr as *const u8, stackmap_size)
        };

        match StackMap::parse(stackmap_data) {
            Ok(sm) => {
                println!("   Parsed {} stackmap records", sm.records.len());
                for (i, record) in sm.records.iter().enumerate() {
                    let gc_locs = sm.get_gc_locations(record);
                    if !gc_locs.is_empty() {
                        println!("     Record {}: offset={:#x}, {} GC pointers",
                                 i, record.instruction_offset, gc_locs.len());
                    }
                }
                // Load into MMTk binding
                mmtk_binding::load_stackmaps(sm, fn_addr);
                println!("   ✓ Stackmaps loaded into MMTk");
            }
            Err(e) => {
                println!("   ✗ Failed to parse stackmaps: {:?}", e);
            }
        }
    } else {
        println!("   ✗ No stackmaps section found!");
    }

    // ===== Execute =====
    println!("\n10. Executing JIT'd function...");
    println!("────────────────────────────────────────────────────────────────────");

    let result = unsafe { build_list.call() };

    println!("────────────────────────────────────────────────────────────────────");

    // ===== Verify Results =====
    println!("\n11. Verifying results...");
    println!("   Returned value: {:#x}", result);

    if is_heap_ptr(result) {
        println!("   List: {}", TaggedValueDisplay(result));

        let v1 = fixnum_value(car(result));
        let v2 = fixnum_value(car(cdr(result)));
        let v3 = fixnum_value(car(cdr(cdr(result))));

        println!("   Elements: {}, {}, {}", v1, v2, v3);

        if v1 == 1 && v2 == 2 && v3 == 3 {
            println!("\n╔══════════════════════════════════════════════════════════════════╗");
            println!("║   ✓ SUCCESS: MMTk + LLVM Statepoints Integration Works!          ║");
            println!("╠══════════════════════════════════════════════════════════════════╣");
            println!("║   • JIT compilation with inkwell                                 ║");
            println!("║   • LLVM statepoints for precise root tracking                   ║");
            println!("║   • MMTk for memory management                                   ║");
            println!("║   • Stackmap-based root discovery                                ║");
            println!("╚══════════════════════════════════════════════════════════════════╝");
        } else {
            println!("\n   ✗ FAIL: Unexpected values in list");
        }
    } else {
        println!("   ✗ FAIL: Result is not a heap pointer");
    }
}

/// Demo the tiny language with lang! macro
fn run_lang_demo() {
    use inkwell::context::Context;
    use inkwell::execution_engine::{MCJITMemoryManager, MCJITMemoryManagerCallbacks};
    use inkwell::passes::PassBuilderOptions;
    use inkwell::targets::{InitializationConfig, Target, TargetMachine, CodeModel, RelocMode};
    use inkwell::OptimizationLevel;
    use std::sync::Mutex;
    use crate::stackmap::StackMap;
    use crate::lang::Compiler;

    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║   Tiny Language Demo with MMTk GC                                ║");
    println!("╚══════════════════════════════════════════════════════════════════╝\n");

    // ===== Program to compile =====
    // Loop test: count down from 3, build list, GC each iteration
    let program = lang!(
        (let result nil
            (let i 3
                (do
                    (while (< 0 i)
                        (do
                            (set result (cons i result))
                            (gc)
                            (set i (- i 1))))
                    (print-list result)
                    result)))
    );

    println!("Program AST: {:?}\n", program);

    // ===== Memory Manager for Stackmaps =====
    struct MemoryManagerState {
        stackmaps: Option<(usize, usize)>,
        code_sections: Vec<(usize, usize)>,
    }

    static MM_STATE: Mutex<MemoryManagerState> = Mutex::new(MemoryManagerState {
        stackmaps: None,
        code_sections: Vec::new(),
    });

    extern "C" fn allocate_code_section(
        _opaque: *mut libc::c_void,
        size: libc::uintptr_t,
        alignment: libc::c_uint,
        _section_id: libc::c_uint,
        _section_name: *const libc::c_char,
    ) -> *mut u8 {
        let size = std::cmp::max(size, alignment as usize);
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
        _is_read_only: libc::c_int,
    ) -> *mut u8 {
        let name = unsafe { std::ffi::CStr::from_ptr(section_name).to_str().unwrap_or("") };
        let size = std::cmp::max(size, alignment as usize);
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
        if name == "__llvm_stackmaps" || name == ".llvm_stackmaps" {
            if let Ok(mut state) = MM_STATE.lock() {
                state.stackmaps = Some((ptr as usize, size));
            }
        }
        ptr
    }

    extern "C" fn finalize_memory(
        _opaque: *mut libc::c_void,
        _err_msg: *mut *mut libc::c_char,
    ) -> i32 {
        if let Ok(state) = MM_STATE.lock() {
            for (addr, size) in &state.code_sections {
                unsafe {
                    libc::mprotect(*addr as *mut libc::c_void, *size, libc::PROT_READ | libc::PROT_EXEC);
                }
            }
        }
        0
    }

    extern "C" fn destroy(_opaque: *mut libc::c_void) {}

    // ===== Initialize LLVM =====
    Target::initialize_native(&InitializationConfig::default())
        .expect("Failed to initialize native target");

    // ===== Initialize MMTk =====
    mmtk_binding::init_mmtk();
    mmtk_binding::bind_mutator();

    // Get runtime symbols
    let symbols = mmtk_binding::get_all_runtime_symbols();
    println!("Runtime symbols:");
    println!("  cons: {:#x}", symbols.cons);
    println!("  gc:   {:#x}", symbols.gc);
    println!("  car:  {:#x}", symbols.car);
    println!("  cdr:  {:#x}", symbols.cdr);
    println!("  print: {:#x}", symbols.print);
    println!("  print_list: {:#x}", symbols.print_list);

    // ===== Compile Program =====
    println!("\nCompiling...");
    let context = Context::create();
    let mut compiler = Compiler::new(&context, "lang_demo");
    compiler.compile_function("main", &program);

    let (module, rt_fns) = compiler.finish();
    println!("Generated IR:");
    module.print_to_stderr();

    // ===== Run Statepoint Pass =====
    println!("\nRunning RewriteStatepointsForGC pass...");
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

    println!("\nIR after statepoint pass:");
    module.print_to_stderr();

    // ===== Create MCJIT =====
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

    use inkwell::execution_engine::ExecutionEngine;
    let ee = unsafe {
        ExecutionEngine::create_mcjit_with_memory_manager(&module, memory_manager, None)
            .expect("Failed to create MCJIT")
    };

    // Add runtime function mappings
    ee.add_global_mapping(&rt_fns.cons_fn, symbols.cons);
    ee.add_global_mapping(&rt_fns.gc_fn, symbols.gc);
    ee.add_global_mapping(&rt_fns.car_fn, symbols.car);
    ee.add_global_mapping(&rt_fns.cdr_fn, symbols.cdr);
    ee.add_global_mapping(&rt_fns.print_fn, symbols.print);
    ee.add_global_mapping(&rt_fns.print_list_fn, symbols.print_list);

    // Get function pointer
    let main_fn = ee.get_function_address("main").expect("No main function");
    println!("\nmain() JIT'd at {:#x}", main_fn);

    // Load stackmaps
    let (stackmap_addr, stackmap_size) = MM_STATE.lock().unwrap().stackmaps.unwrap();
    let stackmap_data = unsafe { std::slice::from_raw_parts(stackmap_addr as *const u8, stackmap_size) };
    let stackmap = StackMap::parse(stackmap_data).expect("Failed to parse stackmap");
    println!("Loaded {} stackmap records", stackmap.records.len());
    mmtk_binding::load_stackmaps(stackmap, main_fn);

    // Enable MMTk collection BEFORE execution so GC can actually happen
    mmtk_binding::enable_collection();

    // ===== Execute =====
    println!("\n═══ Executing ═══\n");
    let main: extern "C" fn() -> u64 = unsafe { std::mem::transmute(main_fn) };
    let result = main();

    println!("\nResult: {:#x}", result);

    println!("\n═══ Demo Complete ═══");
}
