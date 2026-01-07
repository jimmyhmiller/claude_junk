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
mod gc_runtime;
#[macro_use]
mod lang;

fn run() {
    use inkwell::context::Context;
    use inkwell::execution_engine::{ExecutionEngine, MCJITMemoryManager, MCJITMemoryManagerCallbacks};
    use inkwell::passes::PassBuilderOptions;
    use inkwell::targets::{InitializationConfig, Target, TargetMachine, CodeModel, RelocMode};
    use inkwell::OptimizationLevel;
    use std::sync::Mutex;
    use crate::stackmap::StackMap;
    use crate::lang::Compiler;

    let use_fib_banner = std::env::var("STATEPOINT_FIB").is_ok();
    if use_fib_banner {
        println!("╔══════════════════════════════════════════════════════════════════╗");
        println!("║   Fibonacci Benchmark                                            ║");
        println!("╚══════════════════════════════════════════════════════════════════╝\n");
    } else {
        println!("╔══════════════════════════════════════════════════════════════════╗");
        println!("║   Binary Trees Benchmark with MMTk GC                            ║");
        println!("╚══════════════════════════════════════════════════════════════════╝\n");
    }

    let use_mini = std::env::var("STATEPOINT_MINI").is_ok();

    // ===== Binary Trees Benchmark =====
    // This is the exact benchmark from:
    // https://benchmarksgame-team.pages.debian.net/benchmarksgame/program/binarytrees-node-7.html
    //
    // Translated to our Lisp-like language:
    // - TreeNode(left, right) = (cons left right)
    // - node.left = (car node)
    // - node.right = (cdr node)
    // - node.left === null becomes (null? (car node))
    let use_simple = std::env::var("STATEPOINT_SIMPLE").is_ok();
    let use_fib = std::env::var("STATEPOINT_FIB").is_ok();

    let program = if use_fib {
        // Fibonacci benchmark - pure computation (no GC allocations)
        // Tests recursive function call overhead and JIT quality
        lang!(
            (do
                (defn fib (n)
                    (if (<= n 1)
                        n
                        (+ (call fib (- n 1))
                           (call fib (- n 2)))))

                // Get N from command line (default 35)
                (let n (max 1 (get-arg 1))
                    (do
                        (print (call fib n))))))
    } else if use_simple {
        // Test with nested cons cells - requires GC to update heap pointers
        lang!(
            (do
                // Create nested structure: ((1 . 2) . (3 . 4))
                (let inner1 (cons 1 2)
                    (let inner2 (cons 3 4)
                        (let outer (cons inner1 inner2)
                            (do
                                // Verify structure before GC
                                (print (car (car outer)))  // Should print 1
                                (print (cdr (car outer)))  // Should print 2
                                (print (car (cdr outer)))  // Should print 3
                                (print (cdr (cdr outer)))  // Should print 4
                                (set-global-root outer)
                                (gc)  // Force GC - pointers should be updated
                                // Verify structure after GC
                                (print (car (car outer)))  // Should print 1
                                (print (cdr (car outer)))  // Should print 2
                                (print (car (cdr outer)))  // Should print 3
                                (print (cdr (cdr outer)))  // Should print 4
                                (clear-global-root)))))))
    } else if use_mini {
        // STATEPOINT_MINI_DEPTH controls the tree depth (default 1)
        lang!(
            (do
                (defn bottom_up_tree (depth)
                    (if (> depth 0)
                        (cons (call bottom_up_tree (- depth 1))
                              (call bottom_up_tree (- depth 1)))
                        (cons nil nil)))

                (defn item_check (node)
                    (if (null? (car node))
                        1
                        (+ 1 (+ (call item_check (car node))
                                (call item_check (cdr node))))))

                (let treeDepth (get-arg 1)
                    (let longLivedTree (call bottom_up_tree treeDepth)
                        (do
                            (set-global-root longLivedTree)
                            (gc)
                            (print-long-lived-check treeDepth
                                (call item_check longLivedTree))
                            (clear-global-root))))))
    } else {
        lang!(
            (do
                // bottom_up_tree(depth): creates a tree of given depth
                (defn bottom_up_tree (depth)
                    (if (> depth 0)
                        (cons (call bottom_up_tree (- depth 1))
                              (call bottom_up_tree (- depth 1)))
                        (cons nil nil)))

                // item_check(node): counts nodes in tree
                (defn item_check (node)
                    (if (null? (car node))
                        1
                        (+ 1 (+ (call item_check (car node))
                                (call item_check (cdr node))))))

                // work(iterations, depth, longLivedTree): run iterations and print result
                (defn work (iterations depth longLivedTree)
                    (let check 0
                        (let i 0
                            (do
                                (while (< i iterations)
                                    (do
                                        (set check (+ check (call item_check (call bottom_up_tree depth))))
                                        (set i (+ i 1))))
                                (print-trees-check iterations depth check)))))

                // Main benchmark - maxDepth = max(6, argv[1])
                (let maxDepth (max 6 (get-arg 1))
                    (let stretchDepth (+ maxDepth 1)
                        (do
                            // Stretch tree
                            (print-stretch-check stretchDepth
                                (call item_check (call bottom_up_tree stretchDepth)))

                            // Long lived tree - kept alive during work loop
                            (let longLivedTree (call bottom_up_tree maxDepth)
                                (do
                                    (set-global-root longLivedTree)
                                    // Work loop: depth from 4 to maxDepth by 2
                                    (let depth 4
                                        (while (<= depth maxDepth)
                                            (do
                                                (let iterations (<< 1 (+ (- maxDepth depth) 4))
                                                    (call work iterations depth longLivedTree))
                                                (set depth (+ depth 2)))))

                                    // Final check on long-lived tree
                                    (print-long-lived-check maxDepth
                                        (call item_check longLivedTree))
                                    (clear-global-root)))))))
        )
    };

    // println!("Program AST: {:?}\n", program);

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

    // ===== Initialize GC =====
    let use_simple_gc = std::env::var("SIMPLE_GC").is_ok();

    let simple_symbols;
    let mmtk_symbols;

    if use_simple_gc {
        gc_runtime::init();
        simple_symbols = Some(gc_runtime::get_simple_runtime_symbols());
        mmtk_symbols = None;
    } else {
        mmtk_binding::init_mmtk();
        mmtk_binding::bind_mutator();
        mmtk_symbols = Some(mmtk_binding::get_all_runtime_symbols());
        simple_symbols = None;
    }

    // ===== Compile Program =====
    println!("Compiling...");
    let context = Context::create();
    let mut compiler = Compiler::new_with_gc_mode(&context, "lang_demo", use_simple_gc);
    compiler.compile_function("main", &program);

    let (module, rt_fns) = compiler.finish();

    // ===== Run Statepoint Pass =====
    let target_triple = TargetMachine::get_default_triple();
    let target = Target::from_triple(&target_triple).unwrap();
    let opt_level = match std::env::var("STATEPOINT_OPT").as_deref() {
        Ok("less") => OptimizationLevel::Less,
        Ok("default") => OptimizationLevel::Default,
        Ok("none") => OptimizationLevel::None,
        _ => OptimizationLevel::Aggressive,
    };

    let target_machine = target
        .create_target_machine(
            &target_triple,
            "generic",
            "",
            opt_level,
            RelocMode::PIC,
            CodeModel::Default,
        )
        .unwrap();

    module
        .run_passes("rewrite-statepoints-for-gc", &target_machine, PassBuilderOptions::create())
        .expect("Failed to run statepoint pass");

    // Write LLVM IR after statepoint rewriting
    if std::env::var("STATEPOINT_DUMP_IR").is_ok() {
        let ir = module.print_to_string().to_string();
        let ir_path = std::env::var("STATEPOINT_IR_PATH").unwrap_or_else(|_| "statepoints.ll".to_string());
        if let Err(err) = std::fs::write(&ir_path, ir.as_bytes()) {
            eprintln!("Failed to write LLVM IR to {}: {}", ir_path, err);
        }
        if std::env::var("STATEPOINT_PRINT_IR").is_ok() {
            eprintln!("=== LLVM IR AFTER STATEPOINTS ===");
            eprintln!("{}", ir);
        }
    }

    // Print assembly only when requested
    if std::env::var("STATEPOINT_PRINT_ASM").is_ok() {
        use inkwell::targets::FileType;
        let asm = target_machine.write_to_memory_buffer(&module, FileType::Assembly).unwrap();
        eprintln!("=== GENERATED ASSEMBLY ===");
        eprintln!("{}", std::str::from_utf8(asm.as_slice()).unwrap_or("(non-utf8)"));
    }

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
    if let Some(ref symbols) = mmtk_symbols {
        ee.add_global_mapping(&rt_fns.cons_fn, symbols.cons);
        ee.add_global_mapping(&rt_fns.cons_safepoint_fn, symbols.cons_safepoint);
        ee.add_global_mapping(&rt_fns.gc_fn, symbols.gc);
        ee.add_global_mapping(&rt_fns.car_fn, symbols.car);
        ee.add_global_mapping(&rt_fns.cdr_fn, symbols.cdr);
        ee.add_global_mapping(&rt_fns.set_global_root_fn, symbols.set_global_root);
        ee.add_global_mapping(&rt_fns.clear_global_root_fn, symbols.clear_global_root);
        ee.add_global_mapping(&rt_fns.print_fn, symbols.print);
        ee.add_global_mapping(&rt_fns.print_list_fn, symbols.print_list);
        ee.add_global_mapping(&rt_fns.print_stretch_check_fn, symbols.print_stretch_check);
        ee.add_global_mapping(&rt_fns.print_trees_check_fn, symbols.print_trees_check);
        ee.add_global_mapping(&rt_fns.print_long_lived_check_fn, symbols.print_long_lived_check);
        ee.add_global_mapping(&rt_fns.get_arg_fn, symbols.get_arg);
        ee.add_global_mapping(&rt_fns.max_fn, symbols.max);
    } else if let Some(ref symbols) = simple_symbols {
        // Simple GC uses try_alloc_cons which returns NULL if heap is full
        ee.add_global_mapping(&rt_fns.try_alloc_cons_fn, symbols.try_alloc_cons);
        ee.add_global_mapping(&rt_fns.gc_fn, symbols.gc);
        ee.add_global_mapping(&rt_fns.gc_with_frame_info_fn, symbols.gc_with_frame_info);
        ee.add_global_mapping(&rt_fns.car_fn, symbols.car);
        ee.add_global_mapping(&rt_fns.cdr_fn, symbols.cdr);
        ee.add_global_mapping(&rt_fns.set_global_root_fn, symbols.set_global_root);
        ee.add_global_mapping(&rt_fns.clear_global_root_fn, symbols.clear_global_root);
        // Simple GC uses the same print/utility functions from mmtk_binding
        ee.add_global_mapping(&rt_fns.print_fn, mmtk_binding::rt_print as *const () as usize);
        ee.add_global_mapping(&rt_fns.print_list_fn, mmtk_binding::rt_print_list as *const () as usize);
        ee.add_global_mapping(&rt_fns.print_stretch_check_fn, mmtk_binding::rt_print_stretch_check as *const () as usize);
        ee.add_global_mapping(&rt_fns.print_trees_check_fn, mmtk_binding::rt_print_trees_check as *const () as usize);
        ee.add_global_mapping(&rt_fns.print_long_lived_check_fn, mmtk_binding::rt_print_long_lived_check as *const () as usize);
        ee.add_global_mapping(&rt_fns.get_arg_fn, mmtk_binding::rt_get_arg as *const () as usize);
        ee.add_global_mapping(&rt_fns.max_fn, mmtk_binding::rt_max as *const () as usize);
    }

    // Get function pointer
    let main_fn = ee.get_function_address("main").expect("No main function");

    // Load stackmaps - both MMTk and simple GC need them for stack walking
    let stackmap_info = MM_STATE.lock().unwrap().stackmaps;
    if let Some((stackmap_addr, stackmap_size)) = stackmap_info {
        let stackmap_data = unsafe { std::slice::from_raw_parts(stackmap_addr as *const u8, stackmap_size) };
        let mut stackmap = StackMap::parse(stackmap_data).expect("Failed to parse stackmap");
        let code_sections = MM_STATE.lock().unwrap().code_sections.clone();
        let mut is_absolute = false;
        for func in &stackmap.functions {
            if code_sections.iter().any(|(addr, size)| {
                let start = *addr as u64;
                let end = start + *size as u64;
                func.address >= start && func.address < end
            }) {
                is_absolute = true;
                break;
            }
        }
        if !is_absolute {
            let mut base = None;
            for (addr, size) in &code_sections {
                let start = *addr as u64;
                let end = start + *size as u64;
                if (main_fn as u64) >= start && (main_fn as u64) < end {
                    base = Some(start);
                    break;
                }
            }
            let base = base.or_else(|| code_sections.iter().map(|(addr, _)| *addr as u64).min()).unwrap_or(0);
            if base != 0 {
                for func in &mut stackmap.functions {
                    func.address += base;
                }
                for record in &mut stackmap.records {
                    record.absolute_offset += base;
                }
            }
        }

        if use_simple_gc {
            // Load stackmaps for simple GC stack walking
            gc_runtime::load_stackmaps(stackmap, main_fn);
        } else {
            mmtk_binding::load_stackmaps(stackmap, main_fn);
            // Enable MMTk collection BEFORE execution so GC can actually happen
            mmtk_binding::enable_collection();
        }
    } else if !use_simple_gc {
        // MMTk without stackmaps - still enable collection
        mmtk_binding::enable_collection();
    }

    // ===== Execute =====
    println!("Running benchmark...\n");
    let gc_stats = std::env::var("GC_STATS").is_ok() && !use_simple_gc;
    if gc_stats {
        mmtk_binding::start_gc_stats();
    }
    let jit_main: extern "C" fn() -> u64 = unsafe { std::mem::transmute(main_fn) };
    let start_time = std::time::Instant::now();
    let _result = jit_main();
    let elapsed = start_time.elapsed();
    if gc_stats {
        mmtk_binding::end_gc_stats();
    }

    println!("\nElapsed: {:.3}ms", elapsed.as_secs_f64() * 1000.0);
    println!("Done.");
}

fn main() {
    let stack_bytes: usize = std::env::var("STATEPOINT_STACK_BYTES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(256 * 1024 * 1024);

    std::thread::Builder::new()
        .name("statepoints-main".to_string())
        .stack_size(stack_bytes)
        .spawn(run)
        .expect("Failed to spawn main thread")
        .join()
        .expect("Main thread panicked");
}
