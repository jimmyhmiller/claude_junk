use inkwell::context::Context;
use inkwell::intrinsics::Intrinsic;
use inkwell::module::Linkage;
use inkwell::AddressSpace;

/// Demonstrates LLVM statepoint-based GC with inkwell
///
/// Key finding: inkwell doesn't support LLVM's `token` type directly,
/// which is what gc.statepoint returns. So we have two approaches:
///
/// 1. Use RewriteStatepointsForGC pass (recommended) - write "abstract" IR
///    with regular calls, and let the pass transform them to statepoints
///
/// 2. Use llvm-sys directly for the statepoint calls (more complex)
///
/// This demo uses approach #1: the pass-based workflow

fn main() {
    println!("=== LLVM Statepoints with Inkwell ===\n");

    // Check intrinsic availability
    check_intrinsics();

    // Demonstrate the abstract machine model approach
    let lowered_ir = demonstrate_abstract_machine_model();

    // Generate object code with stack maps
    if lowered_ir.is_some() {
        generate_object_with_stackmaps();
    }
}

fn check_intrinsics() {
    println!("=== Checking GC Intrinsics ===\n");

    let intrinsics = [
        "llvm.experimental.gc.statepoint",
        "llvm.experimental.gc.result",
        "llvm.experimental.gc.relocate",
    ];

    for name in &intrinsics {
        match Intrinsic::find(name) {
            Some(i) => println!("✓ {}: overloaded={}", name, i.is_overloaded()),
            None => println!("✗ {} not found", name),
        }
    }

    println!();
    println!("NOTE: inkwell doesn't support LLVM's `token` type, which gc.statepoint returns.");
    println!("      We'll use RewriteStatepointsForGC pass instead of manual construction.\n");
}

/// Build IR in the "abstract machine model" - regular calls that the pass transforms
fn demonstrate_abstract_machine_model() -> Option<String> {
    println!("=== Building Abstract Machine Model IR ===\n");

    let context = Context::create();
    let module = context.create_module("gc_demo");
    let builder = context.create_builder();

    let i64_type = context.i64_type();
    let i32_type = context.i32_type();
    let gc_ptr_type = context.ptr_type(AddressSpace::from(1)); // GC pointers in addrspace 1

    // External GC allocator
    let alloc_fn_type = gc_ptr_type.fn_type(&[i64_type.into()], false);
    let alloc_fn = module.add_function("gc_alloc", alloc_fn_type, Some(Linkage::External));

    // A function to use an object (forces it to stay live)
    let use_fn_type = context.void_type().fn_type(&[gc_ptr_type.into()], false);
    let use_fn = module.add_function("use_object", use_fn_type, Some(Linkage::External));

    // =========================================================================
    // Example 1: Simple case - GC pointer live across a call
    // =========================================================================
    {
        let fn_type = i64_type.fn_type(&[], false);
        let function = module.add_function("simple_gc_example", fn_type, None);
        function.set_gc("statepoint-example");

        let entry = context.append_basic_block(function, "entry");
        builder.position_at_end(entry);

        // Allocate first object
        let size = i64_type.const_int(64, false);
        let obj1 = builder
            .build_call(alloc_fn, &[size.into()], "obj1")
            .unwrap()
            .try_as_basic_value()
            .left()
            .unwrap()
            .into_pointer_value();

        // This call is a potential safepoint - GC might move obj1!
        let _obj2 = builder.build_call(alloc_fn, &[size.into()], "obj2").unwrap();

        // Use obj1 after the potential GC
        // The pass will insert gc.relocate to get the new address
        builder.build_call(use_fn, &[obj1.into()], "").unwrap();

        let val = builder.build_load(i64_type, obj1, "val").unwrap();
        builder.build_return(Some(&val)).unwrap();
    }

    // =========================================================================
    // Example 2: Loop with GC allocations
    // =========================================================================
    {
        let fn_type = context.void_type().fn_type(&[i32_type.into()], false);
        let function = module.add_function("loop_gc_example", fn_type, None);
        function.set_gc("statepoint-example");

        let entry = context.append_basic_block(function, "entry");
        let loop_header = context.append_basic_block(function, "loop.header");
        let loop_body = context.append_basic_block(function, "loop.body");
        let exit = context.append_basic_block(function, "exit");

        // Entry: allocate initial object
        builder.position_at_end(entry);
        let n = function.get_first_param().unwrap().into_int_value();
        let size = i64_type.const_int(32, false);
        let initial_obj = builder
            .build_call(alloc_fn, &[size.into()], "initial_obj")
            .unwrap()
            .try_as_basic_value()
            .left()
            .unwrap()
            .into_pointer_value();
        let zero = i32_type.const_int(0, false);
        builder.build_unconditional_branch(loop_header).unwrap();

        // Loop header: phi nodes for counter and object
        builder.position_at_end(loop_header);
        let i_phi = builder.build_phi(i32_type, "i").unwrap();
        let obj_phi = builder.build_phi(gc_ptr_type, "obj").unwrap();
        i_phi.add_incoming(&[(&zero, entry)]);
        obj_phi.add_incoming(&[(&initial_obj, entry)]);

        let i_val = i_phi.as_basic_value().into_int_value();
        let cond = builder
            .build_int_compare(inkwell::IntPredicate::SLT, i_val, n, "cond")
            .unwrap();
        builder
            .build_conditional_branch(cond, loop_body, exit)
            .unwrap();

        // Loop body: use current object, allocate new one
        builder.position_at_end(loop_body);
        let current_obj = obj_phi.as_basic_value().into_pointer_value();

        // Use the object - forces it to be live
        builder.build_call(use_fn, &[current_obj.into()], "").unwrap();

        // Allocate new object (safepoint!) - current_obj might move
        let new_obj = builder
            .build_call(alloc_fn, &[size.into()], "new_obj")
            .unwrap()
            .try_as_basic_value()
            .left()
            .unwrap()
            .into_pointer_value();

        let one = i32_type.const_int(1, false);
        let i_next = builder.build_int_add(i_val, one, "i.next").unwrap();
        builder.build_unconditional_branch(loop_header).unwrap();

        i_phi.add_incoming(&[(&i_next, loop_body)]);
        obj_phi.add_incoming(&[(&new_obj, loop_body)]);

        // Exit
        builder.position_at_end(exit);
        builder.build_return(None).unwrap();
    }

    // =========================================================================
    // Example 3: Multiple live pointers
    // =========================================================================
    {
        let fn_type = i64_type.fn_type(&[], false);
        let function = module.add_function("multi_pointer_example", fn_type, None);
        function.set_gc("statepoint-example");

        let entry = context.append_basic_block(function, "entry");
        builder.position_at_end(entry);

        let size = i64_type.const_int(64, false);

        // Allocate three objects
        let obj1 = builder
            .build_call(alloc_fn, &[size.into()], "obj1")
            .unwrap()
            .try_as_basic_value()
            .left()
            .unwrap()
            .into_pointer_value();

        let obj2 = builder
            .build_call(alloc_fn, &[size.into()], "obj2")
            .unwrap()
            .try_as_basic_value()
            .left()
            .unwrap()
            .into_pointer_value();

        let obj3 = builder
            .build_call(alloc_fn, &[size.into()], "obj3")
            .unwrap()
            .try_as_basic_value()
            .left()
            .unwrap()
            .into_pointer_value();

        // All three pointers are live across this call
        // The pass must insert relocates for all of them
        let _obj4 = builder.build_call(alloc_fn, &[size.into()], "obj4").unwrap();

        // Use all three
        let v1 = builder.build_load(i64_type, obj1, "v1").unwrap();
        let v2 = builder.build_load(i64_type, obj2, "v2").unwrap();
        let v3 = builder.build_load(i64_type, obj3, "v3").unwrap();

        let sum1 = builder
            .build_int_add(v1.into_int_value(), v2.into_int_value(), "sum1")
            .unwrap();
        let sum2 = builder.build_int_add(sum1, v3.into_int_value(), "sum2").unwrap();

        builder.build_return(Some(&sum2)).unwrap();
    }

    let ir = module.print_to_string().to_string();
    println!("Generated IR (abstract model):\n{}", ir);

    // Write to file
    module.print_to_file("abstract_gc.ll").unwrap();
    println!("Written to abstract_gc.ll\n");

    // Run the RewriteStatepointsForGC pass
    println!("=== Running RewriteStatepointsForGC Pass ===\n");

    run_rewrite_statepoints_pass()
}

fn run_rewrite_statepoints_pass() -> Option<String> {
    let output = std::process::Command::new("opt")
        .args(["-passes=rewrite-statepoints-for-gc", "abstract_gc.ll", "-S"])
        .output();

    match output {
        Ok(result) => {
            if result.status.success() {
                let ir = String::from_utf8_lossy(&result.stdout).to_string();
                println!("Transformed IR (with statepoints):\n{}", ir);
                std::fs::write("lowered_gc.ll", &ir).unwrap();
                println!("Written to lowered_gc.ll\n");
                Some(ir)
            } else {
                println!("opt failed: {}", String::from_utf8_lossy(&result.stderr));
                None
            }
        }
        Err(e) => {
            println!("Failed to run opt: {}", e);
            None
        }
    }
}

fn generate_object_with_stackmaps() {
    println!("=== Generating Object Code with Stack Maps ===\n");

    // Compile to object file
    let llc_output = std::process::Command::new("llc")
        .args(["-filetype=obj", "lowered_gc.ll", "-o", "gc_demo.o"])
        .output();

    match llc_output {
        Ok(result) => {
            if result.status.success() {
                println!("Generated gc_demo.o");

                // Read stack maps
                println!("\n=== Stack Map Contents ===\n");
                let readobj_output = std::process::Command::new("llvm-readobj")
                    .args(["--stackmap", "gc_demo.o"])
                    .output();

                match readobj_output {
                    Ok(res) => {
                        if res.status.success() {
                            println!("{}", String::from_utf8_lossy(&res.stdout));
                        } else {
                            // Try alternate tools
                            let readelf_output = std::process::Command::new("readelf")
                                .args(["-x", ".llvm_stackmaps", "gc_demo.o"])
                                .output();

                            match readelf_output {
                                Ok(r) => {
                                    if r.status.success() {
                                        println!("Raw .llvm_stackmaps section:");
                                        println!("{}", String::from_utf8_lossy(&r.stdout));
                                    } else {
                                        println!("No stack maps found or error reading");
                                    }
                                }
                                Err(e) => println!("readelf failed: {}", e),
                            }
                        }
                    }
                    Err(e) => println!("llvm-readobj failed: {}", e),
                }
            } else {
                println!("llc failed: {}", String::from_utf8_lossy(&result.stderr));
            }
        }
        Err(e) => println!("Failed to run llc: {}", e),
    }

    // Also generate assembly to see the actual code
    println!("\n=== Generated Assembly (excerpt) ===\n");
    let llc_asm = std::process::Command::new("llc")
        .args(["lowered_gc.ll", "-o", "-"])
        .output();

    match llc_asm {
        Ok(result) => {
            if result.status.success() {
                let asm = String::from_utf8_lossy(&result.stdout);
                // Print first 100 lines
                for (i, line) in asm.lines().enumerate() {
                    if i > 100 {
                        println!("... (truncated)");
                        break;
                    }
                    println!("{}", line);
                }
            }
        }
        Err(_) => {}
    }
}
