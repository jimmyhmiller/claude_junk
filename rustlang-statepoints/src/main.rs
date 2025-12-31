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
mod mmtk_binding;
#[macro_use]
mod lang;

fn main() {
    use inkwell::context::Context;
    use inkwell::execution_engine::{ExecutionEngine, MCJITMemoryManager, MCJITMemoryManagerCallbacks};
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
    let jit_main: extern "C" fn() -> u64 = unsafe { std::mem::transmute(main_fn) };
    let result = jit_main();

    println!("\nResult: {:#x}", result);

    println!("\n═══ Demo Complete ═══");
}
