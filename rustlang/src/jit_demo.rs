//! JIT Compilation Demo with Shadow Stack GC
//!
//! This demonstrates a complete working solution for a dynamic language with:
//! 1. Tagged pointers (fixnums, characters, heap pointers)
//! 2. Shadow stack for GC root tracking
//! 3. LLVM JIT compilation
//! 4. Proper GC that moves objects and updates roots

use inkwell::context::Context;
use inkwell::module::Linkage;
use inkwell::targets::{InitializationConfig, Target, TargetMachine, RelocMode, CodeModel};
use inkwell::OptimizationLevel;
use inkwell::execution_engine::JitFunction;

use crate::tagged_value::*;
use crate::tagged_gc;
use crate::shadow_stack::{self, ShadowFrame};

/// Run the complete JIT demo
pub fn run_demo() {
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║   WORKING JIT COMPILER WITH SHADOW STACK GC                  ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    // Part 1: Demonstrate shadow stack working from Rust
    demonstrate_shadow_stack_rust();

    // Part 2: Generate and execute LLVM IR with shadow stack
    demonstrate_jit_with_gc();
}

fn demonstrate_shadow_stack_rust() {
    println!("═══════════════════════════════════════════════════════════════");
    println!("PART 1: Shadow Stack from Rust");
    println!("═══════════════════════════════════════════════════════════════\n");

    // Reset GC state
    tagged_gc::reset();

    println!("Building list (1 2 3) with proper shadow stack rooting...\n");

    // This is how generated code would work:
    // 1. Push a shadow stack frame with slots for live values
    // 2. After each allocation, save the result to a slot
    // 3. Before using a value after a potential GC point, reload from slot
    // 4. Pop the frame when done

    let result = build_list_with_shadow_stack();

    println!("\nResult: {}", TaggedValueDisplay(result));
    println!("  car = {}", TaggedValueDisplay(car(result)));
    println!("  cadr = {}", TaggedValueDisplay(car(cdr(result))));
    println!("  caddr = {}", TaggedValueDisplay(car(cdr(cdr(result)))));

    // Verify values
    let v1 = fixnum_value(car(result));
    let v2 = fixnum_value(car(cdr(result)));
    let v3 = fixnum_value(car(cdr(cdr(result))));

    if v1 == 1 && v2 == 2 && v3 == 3 {
        println!("\n✓ List correctly built with shadow stack rooting!");
    }

    println!();
    tagged_gc::stats();
}

/// Build a list (1 2 3) using the shadow stack for rooting
///
/// KEY INSIGHT: The shadow stack pattern requires that we:
/// 1. Store values to shadow stack slots BEFORE any allocation
/// 2. Read from shadow stack slots AFTER any allocation that might GC
/// 3. The allocation function (rt_cons) reads its arguments, then may GC,
///    then creates the cons cell. So arguments passed to it are used
///    before GC happens.
///
/// This means the pattern is:
///   frame.set(0, val);           // Save before potential GC
///   result = cons(a, val);       // val is used, then GC may happen, then cons created
///   frame.set(1, result);        // Save the new result
///   val = frame.get(0);          // Reload in case it moved (for later use)
fn build_list_with_shadow_stack() -> TaggedValue {
    // Allocate a shadow stack frame with 3 slots
    let mut frame = ShadowFrame::new(3);
    let _guard = shadow_stack::with_frame(&mut frame);

    tagged_gc::set_verbose(true);

    // cons(3, nil) - first allocation
    let cell3 = tagged_gc::cons(make_fixnum(3), NIL);
    println!("Allocated cell3 = {} at {:#x}", TaggedValueDisplay(cell3), cell3);

    // Save cell3 to slot 0
    frame.set(0, cell3);

    // Force GC to test the system
    println!("\n--- Forcing GC before next allocation ---");
    tagged_gc::collect();

    // Reload cell3 after GC
    let cell3 = frame.get(0);
    println!("After GC: cell3 is at {:#x}", cell3);

    // cons(2, cell3) - cell3 is passed as argument (used before any GC in cons)
    let cell2 = tagged_gc::cons(make_fixnum(2), cell3);
    println!("Allocated cell2 = {} at {:#x}", TaggedValueDisplay(cell2), cell2);

    // Save cell2 to slot 1
    frame.set(1, cell2);

    // Force GC again
    println!("\n--- Forcing GC before next allocation ---");
    tagged_gc::collect();

    // Reload after GC
    let cell3 = frame.get(0);
    let cell2 = frame.get(1);
    println!("After GC: cell3 at {:#x}, cell2 at {:#x}", cell3, cell2);

    // cons(1, cell2)
    let cell1 = tagged_gc::cons(make_fixnum(1), cell2);
    println!("Allocated cell1 = {} at {:#x}", TaggedValueDisplay(cell1), cell1);

    // Save cell1 to slot 2
    frame.set(2, cell1);

    // One more GC just to prove everything survives
    println!("\n--- Final GC ---");
    tagged_gc::collect();

    // Return the list (reload from shadow stack)
    frame.get(2)
}

fn demonstrate_jit_with_gc() {
    println!("\n═══════════════════════════════════════════════════════════════");
    println!("PART 2: JIT Compilation with Shadow Stack");
    println!("═══════════════════════════════════════════════════════════════\n");

    // Initialize LLVM
    Target::initialize_native(&InitializationConfig::default())
        .expect("Failed to initialize native target");

    let context = Context::create();
    let module = context.create_module("jit_demo");
    let builder = context.create_builder();

    // Types
    let i64_type = context.i64_type();
    let i32_type = context.i32_type();
    let void_type = context.void_type();
    let ptr_type = context.ptr_type(inkwell::AddressSpace::default());
    let frame_type = context.struct_type(
        &[
            ptr_type.into(),           // prev pointer
            i32_type.into(),           // num_slots
            i64_type.array_type(32).into(), // slots array
        ],
        false,
    );

    // Declare runtime functions
    let cons_fn = module.add_function(
        "rt_cons",
        i64_type.fn_type(&[i64_type.into(), i64_type.into()], false),
        Some(Linkage::External),
    );

    let gc_fn = module.add_function(
        "rt_gc",
        void_type.fn_type(&[], false),
        Some(Linkage::External),
    );

    let push_fn = module.add_function(
        "gc_shadow_stack_push",
        void_type.fn_type(&[ptr_type.into(), i32_type.into()], false),
        Some(Linkage::External),
    );

    let pop_fn = module.add_function(
        "gc_shadow_stack_pop",
        void_type.fn_type(&[ptr_type.into()], false),
        Some(Linkage::External),
    );

    // Create the build_list function
    println!("Generating build_list() with shadow stack...\n");

    let fn_type = i64_type.fn_type(&[], false);
    let function = module.add_function("build_list", fn_type, None);

    let entry = context.append_basic_block(function, "entry");
    builder.position_at_end(entry);

    // Allocate shadow stack frame on the stack
    let frame_ptr = builder.build_alloca(frame_type, "frame").unwrap();

    // Push the frame with 2 slots
    builder.build_call(push_fn, &[frame_ptr.into(), i32_type.const_int(2, false).into()], "").unwrap();

    // Constants for tagged values
    let nil = i64_type.const_int(NIL, false);
    let one = i64_type.const_int(make_fixnum(1), false);
    let two = i64_type.const_int(make_fixnum(2), false);
    let three = i64_type.const_int(make_fixnum(3), false);

    // cell3 = cons(3, nil)
    let cell3 = builder.build_call(cons_fn, &[three.into(), nil.into()], "cell3")
        .unwrap().try_as_basic_value().left().unwrap().into_int_value();

    // Store cell3 to frame.slots[0]
    let slots_ptr = builder.build_struct_gep(frame_type, frame_ptr, 2, "slots_ptr").unwrap();
    let slot0_ptr = unsafe { builder.build_gep(i64_type.array_type(32), slots_ptr,
        &[i32_type.const_zero(), i32_type.const_zero()], "slot0") }.unwrap();
    builder.build_store(slot0_ptr, cell3).unwrap();

    // Force GC to test the system
    builder.build_call(gc_fn, &[], "").unwrap();

    // Reload cell3 from slot (it may have moved!)
    let cell3_reloaded = builder.build_load(i64_type, slot0_ptr, "cell3_rel").unwrap().into_int_value();

    // cell2 = cons(2, cell3_reloaded)
    let cell2 = builder.build_call(cons_fn, &[two.into(), cell3_reloaded.into()], "cell2")
        .unwrap().try_as_basic_value().left().unwrap().into_int_value();

    // Store cell2 to frame.slots[1]
    let slot1_ptr = unsafe { builder.build_gep(i64_type.array_type(32), slots_ptr,
        &[i32_type.const_zero(), i32_type.const_int(1, false)], "slot1") }.unwrap();
    builder.build_store(slot1_ptr, cell2).unwrap();

    // Force GC again
    builder.build_call(gc_fn, &[], "").unwrap();

    // Reload cell2 from slot
    let cell2_reloaded = builder.build_load(i64_type, slot1_ptr, "cell2_rel").unwrap().into_int_value();

    // cell1 = cons(1, cell2_reloaded)
    let cell1 = builder.build_call(cons_fn, &[one.into(), cell2_reloaded.into()], "cell1")
        .unwrap().try_as_basic_value().left().unwrap().into_int_value();

    // Pop the shadow stack frame
    builder.build_call(pop_fn, &[frame_ptr.into()], "").unwrap();

    // Return the list
    builder.build_return(Some(&cell1)).unwrap();

    // Print the generated IR
    let ir = module.print_to_string().to_string();
    println!("─── Generated LLVM IR ───\n");
    println!("{}", ir);

    // Save IR for inspection
    module.print_to_file("jit_shadow_stack.ll").unwrap();
    println!("Saved to jit_shadow_stack.ll\n");

    // Create execution engine and run
    println!("─── Executing JIT-compiled code ───\n");

    // Reset GC for fresh test
    tagged_gc::reset();
    tagged_gc::set_verbose(true);

    let execution_engine = module
        .create_jit_execution_engine(OptimizationLevel::None)
        .expect("Failed to create execution engine");

    // Add runtime function mappings
    execution_engine.add_global_mapping(&cons_fn, tagged_gc::rt_cons as usize);
    execution_engine.add_global_mapping(&gc_fn, tagged_gc::rt_gc as usize);
    execution_engine.add_global_mapping(&push_fn, shadow_stack::gc_shadow_stack_push as usize);
    execution_engine.add_global_mapping(&pop_fn, shadow_stack::gc_shadow_stack_pop as usize);

    // Get and call the function
    type BuildListFn = unsafe extern "C" fn() -> u64;
    let build_list: JitFunction<BuildListFn> = unsafe {
        execution_engine.get_function("build_list").expect("Failed to get function")
    };

    println!("Calling JIT-compiled build_list()...\n");
    let result = unsafe { build_list.call() };

    println!("\n─── Results ───\n");
    println!("Returned value: {:#x}", result);
    println!("As list: {}", TaggedValueDisplay(result));

    // Verify the list
    if is_cons(result) {
        let v1 = fixnum_value(car(result));
        let v2 = fixnum_value(car(cdr(result)));
        let v3 = fixnum_value(car(cdr(cdr(result))));

        println!("\nList elements: {}, {}, {}", v1, v2, v3);

        if v1 == 1 && v2 == 2 && v3 == 3 {
            println!("\n✓✓✓ JIT-COMPILED CODE WITH GC WORKS CORRECTLY! ✓✓✓");
        } else {
            println!("\n✗ Values incorrect!");
        }
    } else {
        println!("\n✗ Result is not a cons cell!");
    }

    println!();
    tagged_gc::stats();

    // Now stress test with many GC cycles
    println!("\n─── Stress Test: Multiple GC cycles ───\n");
    stress_test_jit(&execution_engine, &build_list);
}

fn stress_test_jit(
    _engine: &inkwell::execution_engine::ExecutionEngine,
    build_list: &JitFunction<unsafe extern "C" fn() -> u64>,
) {
    tagged_gc::set_verbose(false);

    println!("Running 10 iterations of build_list with GC...\n");

    for i in 0..10 {
        let result = unsafe { build_list.call() };

        // Verify each time
        let v1 = fixnum_value(car(result));
        let v2 = fixnum_value(car(cdr(result)));
        let v3 = fixnum_value(car(cdr(cdr(result))));

        if v1 != 1 || v2 != 2 || v3 != 3 {
            println!("✗ Iteration {} failed! Got ({} {} {})", i, v1, v2, v3);
            return;
        }

        // Force extra GC between iterations
        tagged_gc::collect();
    }

    println!("✓ All 10 iterations produced correct results!");
    println!();
    tagged_gc::stats();
}
