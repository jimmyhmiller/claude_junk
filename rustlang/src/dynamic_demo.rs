//! Demonstration of a dynamic language with tagged pointers and GC
//!
//! This module shows:
//! 1. Tagged value representation with immediate values
//! 2. Heap-allocated cons cells that can be moved by GC
//! 3. Proper GC root handling for tagged values
//! 4. LLVM IR generation for dynamic operations

use crate::tagged_value::*;
use crate::tagged_gc;
use inkwell::context::Context;
use inkwell::module::Linkage;
use inkwell::IntPredicate;
use inkwell::AddressSpace;

/// Demonstrate the tagged pointer system working with GC
pub fn run_demo() {
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║   DYNAMIC LANGUAGE WITH TAGGED POINTERS + GC                 ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    // Part 1: Show the tagging scheme
    demonstrate_tagging();

    // Part 2: Show cons cells and lists with GC
    demonstrate_lists_with_gc();

    // Part 3: Generate LLVM IR for dynamic operations
    generate_dynamic_ir();
}

fn demonstrate_tagging() {
    println!("═══════════════════════════════════════════════════════════════");
    println!("PART 1: Tagged Pointer Representation");
    println!("═══════════════════════════════════════════════════════════════\n");

    println!("Tag scheme (lower 3 bits):");
    println!("  000 - Heap pointer (8-byte aligned)");
    println!("  001 - Fixnum (61-bit signed integer)");
    println!("  010 - Character");
    println!("  011 - Special (nil, true, false)");
    println!("  100 - Symbol");
    println!();

    // Demonstrate fixnums
    println!("Fixnums (immediate integers, no allocation):");
    for n in [0i64, 1, -1, 42, 1000000, -999999] {
        let tagged = make_fixnum(n);
        println!("  {} -> {:#018x} (tag: {:03b})",
            n, tagged, tagged & 0b111);
    }
    println!();

    // Demonstrate characters
    println!("Characters (immediate, no allocation):");
    for c in ['a', 'Z', '0', 'λ', '🦀'] {
        let tagged = make_char(c);
        println!("  '{}' -> {:#018x} (tag: {:03b})",
            c, tagged, tagged & 0b111);
    }
    println!();

    // Demonstrate special constants
    println!("Special constants:");
    println!("  nil   -> {:#018x} (tag: {:03b})", NIL, NIL & 0b111);
    println!("  true  -> {:#018x} (tag: {:03b})", TRUE, TRUE & 0b111);
    println!("  false -> {:#018x} (tag: {:03b})", FALSE, FALSE & 0b111);
    println!();

    // Demonstrate truthiness
    println!("Truthiness (Lisp-style: only nil and #f are falsy):");
    let values = [
        ("nil", NIL),
        ("#t", TRUE),
        ("#f", FALSE),
        ("0", make_fixnum(0)),
        ("42", make_fixnum(42)),
        ("'a'", make_char('a')),
    ];
    for (name, v) in values {
        println!("  {} is {}", name, if is_truthy(v) { "truthy" } else { "falsy" });
    }
    println!();

    // Demonstrate arithmetic (no allocation!)
    println!("Arithmetic on fixnums (no allocation):");
    let a = make_fixnum(10);
    let b = make_fixnum(3);
    println!("  {} + {} = {}", TaggedValueDisplay(a), TaggedValueDisplay(b),
        TaggedValueDisplay(arithmetic::add(a, b).unwrap()));
    println!("  {} - {} = {}", TaggedValueDisplay(a), TaggedValueDisplay(b),
        TaggedValueDisplay(arithmetic::sub(a, b).unwrap()));
    println!("  {} * {} = {}", TaggedValueDisplay(a), TaggedValueDisplay(b),
        TaggedValueDisplay(arithmetic::mul(a, b).unwrap()));
    println!("  {} < {} = {}", TaggedValueDisplay(a), TaggedValueDisplay(b),
        TaggedValueDisplay(arithmetic::lt(a, b).unwrap()));
    println!();
}

fn demonstrate_lists_with_gc() {
    println!("═══════════════════════════════════════════════════════════════");
    println!("PART 2: Cons Cells, Lists, and GC Movement");
    println!("═══════════════════════════════════════════════════════════════\n");

    // Initialize GC
    tagged_gc::init();

    // Build a list: (1 2 3)
    println!("Building list (1 2 3)...\n");

    // We need to be careful about GC here!
    // Each cons allocation could trigger GC, so we need proper rooting.

    // Allocate from right to left (3, then 2, then 1)
    let three = make_fixnum(3);
    let two = make_fixnum(2);
    let one = make_fixnum(1);

    // cons(3, nil) - first allocation
    let mut list = tagged_gc::cons(three, NIL);
    println!("  cons(3, nil) = {} at {:#x}", TaggedValueDisplay(list), list);

    // Save list as a root before next allocation
    let mut roots: Vec<*mut TaggedValue> = vec![&mut list as *mut TaggedValue];

    // cons(2, list)
    list = tagged_gc::cons(two, list);
    roots[0] = &mut list;
    println!("  cons(2, (3)) = {} at {:#x}", TaggedValueDisplay(list), list);

    // cons(1, list)
    list = tagged_gc::cons(one, list);
    roots[0] = &mut list;
    println!("  cons(1, (2 3)) = {} at {:#x}", TaggedValueDisplay(list), list);

    // Show the list structure
    println!("\nList structure:");
    println!("  list = {}", TaggedValueDisplay(list));
    println!("  car(list) = {}", TaggedValueDisplay(car(list)));
    println!("  cdr(list) = {}", TaggedValueDisplay(cdr(list)));
    println!("  cadr(list) = {}", TaggedValueDisplay(car(cdr(list))));
    println!("  cddr(list) = {}", TaggedValueDisplay(cdr(cdr(list))));

    // Now demonstrate GC
    println!("\n─────────────────────────────────────────────────────────────────");
    println!("Triggering GC to show that list is properly moved...\n");

    let addr_before = list;

    // Set up a proper shadow stack frame for roots
    use crate::shadow_stack::{ShadowFrame, with_frame};
    let mut frame = ShadowFrame::new(1);
    let _guard = with_frame(&mut frame);
    frame.set(0, list);

    // Collect (GC will walk shadow stack)
    tagged_gc::collect();

    // Reload the list from shadow stack
    list = frame.get(0);

    let addr_after = list;

    println!("Address before GC: {:#x}", addr_before);
    println!("Address after GC:  {:#x}", addr_after);

    if addr_before != addr_after {
        println!("\n✓ LIST WAS MOVED TO A DIFFERENT ADDRESS!");
    }

    // Verify list is still intact
    println!("\nVerifying list contents after GC:");
    println!("  list = {}", TaggedValueDisplay(list));

    let v1 = fixnum_value(car(list));
    let v2 = fixnum_value(car(cdr(list)));
    let v3 = fixnum_value(car(cdr(cdr(list))));

    println!("  Values: {}, {}, {}", v1, v2, v3);

    if v1 == 1 && v2 == 2 && v3 == 3 {
        println!("\n✓✓✓ ALL LIST DATA INTACT AFTER GC! ✓✓✓");
    } else {
        println!("\n✗ Data corrupted!");
    }

    // Build a more complex structure
    println!("\n─────────────────────────────────────────────────────────────────");
    println!("Building nested list ((1 2) (3 4))...\n");

    // ((1 2) (3 4))
    let mut inner1 = tagged_gc::cons(make_fixnum(1),
                     tagged_gc::cons(make_fixnum(2), NIL));
    let mut inner2 = tagged_gc::cons(make_fixnum(3),
                     tagged_gc::cons(make_fixnum(4), NIL));
    let mut nested = tagged_gc::cons(inner1,
                     tagged_gc::cons(inner2, NIL));

    println!("  nested = {}", TaggedValueDisplay(nested));
    println!("  car(nested) = {}", TaggedValueDisplay(car(nested)));
    println!("  cadr(nested) = {}", TaggedValueDisplay(car(cdr(nested))));

    // Multiple GC cycles
    println!("\nRunning 3 GC cycles on nested structure...\n");

    // Set up shadow stack for nested structures
    let mut frame2 = ShadowFrame::new(3);
    let _guard2 = with_frame(&mut frame2);
    frame2.set(0, nested);
    frame2.set(1, inner1);
    frame2.set(2, inner2);

    for i in 1..=3 {
        let before = frame2.get(0);
        tagged_gc::collect();
        nested = frame2.get(0);
        inner1 = frame2.get(1);
        inner2 = frame2.get(2);
        println!("  GC {}: {:#x} -> {:#x}", i, before, nested);
    }

    // Verify still intact
    println!("\nFinal nested structure: {}", TaggedValueDisplay(nested));

    // Check the values
    let a = fixnum_value(car(car(nested)));  // 1
    let b = fixnum_value(car(cdr(car(nested))));  // 2
    let c = fixnum_value(car(car(cdr(nested))));  // 3
    let d = fixnum_value(car(cdr(car(cdr(nested)))));  // 4

    if a == 1 && b == 2 && c == 3 && d == 4 {
        println!("✓✓✓ NESTED STRUCTURE SURVIVED {} GC CYCLES! ✓✓✓", 3);
    }

    println!();
    tagged_gc::stats();
}

fn generate_dynamic_ir() {
    println!("\n═══════════════════════════════════════════════════════════════");
    println!("PART 3: LLVM IR for Dynamic Operations");
    println!("═══════════════════════════════════════════════════════════════\n");

    let context = Context::create();
    let module = context.create_module("dynamic_lang");
    let builder = context.create_builder();

    // Tagged value is i64
    let tagged_type = context.i64_type();
    let gc_ptr_type = context.ptr_type(AddressSpace::from(1));
    let i64_type = context.i64_type();
    let i1_type = context.bool_type();

    // Constants for tags
    let tag_mask = i64_type.const_int(0b111, false);
    let tag_fixnum = i64_type.const_int(tags::FIXNUM, false);
    let tag_heap = i64_type.const_int(tags::HEAP_PTR, false);
    let tag_bits = i64_type.const_int(tags::TAG_BITS, false);
    let nil_const = i64_type.const_int(NIL, false);

    // Declare runtime functions
    let cons_type = tagged_type.fn_type(&[tagged_type.into(), tagged_type.into()], false);
    let cons_fn = module.add_function("rt_cons", cons_type, Some(Linkage::External));

    // Create is_fixnum function
    println!("Generating is_fixnum(v)...");
    let is_fixnum_type = i1_type.fn_type(&[tagged_type.into()], false);
    let is_fixnum_fn = module.add_function("is_fixnum", is_fixnum_type, None);
    {
        let entry = context.append_basic_block(is_fixnum_fn, "entry");
        builder.position_at_end(entry);

        let v = is_fixnum_fn.get_nth_param(0).unwrap().into_int_value();
        let tag = builder.build_and(v, tag_mask, "tag").unwrap();
        let result = builder.build_int_compare(IntPredicate::EQ, tag, tag_fixnum, "is_fix").unwrap();
        builder.build_return(Some(&result)).unwrap();
    }

    // Create is_heap_ptr function
    println!("Generating is_heap_ptr(v)...");
    let is_ptr_type = i1_type.fn_type(&[tagged_type.into()], false);
    let is_ptr_fn = module.add_function("is_heap_ptr", is_ptr_type, None);
    {
        let entry = context.append_basic_block(is_ptr_fn, "entry");
        builder.position_at_end(entry);

        let v = is_ptr_fn.get_nth_param(0).unwrap().into_int_value();
        let tag = builder.build_and(v, tag_mask, "tag").unwrap();
        let is_ptr_tag = builder.build_int_compare(IntPredicate::EQ, tag, tag_heap, "is_ptr").unwrap();
        let is_nonzero = builder.build_int_compare(IntPredicate::NE, v, i64_type.const_zero(), "nonzero").unwrap();
        let result = builder.build_and(is_ptr_tag, is_nonzero, "is_heap").unwrap();
        builder.build_return(Some(&result)).unwrap();
    }

    // Create fixnum_add function
    println!("Generating fixnum_add(a, b)...");
    let add_type = tagged_type.fn_type(&[tagged_type.into(), tagged_type.into()], false);
    let add_fn = module.add_function("fixnum_add", add_type, None);
    {
        let entry = context.append_basic_block(add_fn, "entry");
        builder.position_at_end(entry);

        let a = add_fn.get_nth_param(0).unwrap().into_int_value();
        let b = add_fn.get_nth_param(1).unwrap().into_int_value();

        // For fixnums, we can add directly since the tag bits are the same
        // a + b will give us (a_val << 3 | 001) + (b_val << 3 | 001)
        //                  = (a_val + b_val) << 3 + 2
        // We need to subtract 1 to fix the tag: result - 1
        // OR we can shift both, add, shift back

        // Simpler: shift out tags, add, shift back with tag
        let a_val = builder.build_right_shift(a, tag_bits, true, "a_val").unwrap();
        let b_val = builder.build_right_shift(b, tag_bits, true, "b_val").unwrap();
        let sum = builder.build_int_add(a_val, b_val, "sum").unwrap();
        let shifted = builder.build_left_shift(sum, tag_bits, "shifted").unwrap();
        let result = builder.build_or(shifted, tag_fixnum, "tagged").unwrap();
        builder.build_return(Some(&result)).unwrap();
    }

    // Create a function that builds a list with proper GC handling
    println!("Generating build_list_1_2_3() with GC statepoints...");
    let list_fn_type = tagged_type.fn_type(&[], false);
    let list_fn = module.add_function("build_list_1_2_3", list_fn_type, None);
    list_fn.set_gc("statepoint-example");
    {
        let entry = context.append_basic_block(list_fn, "entry");
        builder.position_at_end(entry);

        // Create fixnum constants (no allocation needed!)
        let one = i64_type.const_int(make_fixnum(1), false);
        let two = i64_type.const_int(make_fixnum(2), false);
        let three = i64_type.const_int(make_fixnum(3), false);
        let nil = i64_type.const_int(NIL, false);

        // Build list right to left: cons(3, nil)
        // This call is a safepoint!
        let cell3 = builder.build_call(cons_fn, &[three.into(), nil.into()], "cell3")
            .unwrap()
            .try_as_basic_value()
            .left()
            .unwrap()
            .into_int_value();

        // cons(2, cell3) - cell3 must survive across this call!
        let cell2 = builder.build_call(cons_fn, &[two.into(), cell3.into()], "cell2")
            .unwrap()
            .try_as_basic_value()
            .left()
            .unwrap()
            .into_int_value();

        // cons(1, cell2) - cell2 (and transitively cell3) must survive
        let cell1 = builder.build_call(cons_fn, &[one.into(), cell2.into()], "cell1")
            .unwrap()
            .try_as_basic_value()
            .left()
            .unwrap()
            .into_int_value();

        builder.build_return(Some(&cell1)).unwrap();
    }

    // Create a function that demonstrates conditional on type
    println!("Generating type_dispatch(v)...");
    let dispatch_type = tagged_type.fn_type(&[tagged_type.into()], false);
    let dispatch_fn = module.add_function("type_dispatch", dispatch_type, None);
    {
        let entry = context.append_basic_block(dispatch_fn, "entry");
        let fixnum_block = context.append_basic_block(dispatch_fn, "is_fixnum");
        let heap_block = context.append_basic_block(dispatch_fn, "is_heap");
        let other_block = context.append_basic_block(dispatch_fn, "other");
        let exit_block = context.append_basic_block(dispatch_fn, "exit");

        builder.position_at_end(entry);
        let v = dispatch_fn.get_nth_param(0).unwrap().into_int_value();
        let tag = builder.build_and(v, tag_mask, "tag").unwrap();

        // Check if fixnum
        let is_fix = builder.build_int_compare(IntPredicate::EQ, tag, tag_fixnum, "is_fix").unwrap();
        builder.build_conditional_branch(is_fix, fixnum_block, heap_block).unwrap();

        // Fixnum path: return v + 1
        builder.position_at_end(fixnum_block);
        let v_val = builder.build_right_shift(v, tag_bits, true, "v_val").unwrap();
        let one = i64_type.const_int(1, false);
        let incremented = builder.build_int_add(v_val, one, "inc").unwrap();
        let fix_result = builder.build_left_shift(incremented, tag_bits, "shifted").unwrap();
        let fix_result = builder.build_or(fix_result, tag_fixnum, "tagged").unwrap();
        builder.build_unconditional_branch(exit_block).unwrap();

        // Heap pointer path: check if heap ptr
        builder.position_at_end(heap_block);
        let is_ptr = builder.build_int_compare(IntPredicate::EQ, tag, tag_heap, "is_heap").unwrap();
        builder.build_conditional_branch(is_ptr, other_block, other_block).unwrap();

        // Other path: return nil
        builder.position_at_end(other_block);
        builder.build_unconditional_branch(exit_block).unwrap();

        // Exit with phi
        builder.position_at_end(exit_block);
        let phi = builder.build_phi(tagged_type, "result").unwrap();
        phi.add_incoming(&[(&fix_result, fixnum_block), (&nil_const, other_block)]);
        builder.build_return(Some(&phi.as_basic_value().into_int_value())).unwrap();
    }

    // Print the IR
    let ir = module.print_to_string().to_string();
    println!("\n─── Generated LLVM IR ───\n");
    println!("{}", ir);

    // Save IR
    module.print_to_file("dynamic_abstract.ll").unwrap();
    println!("\nSaved to dynamic_abstract.ll");

    // Run statepoint pass
    println!("\nRunning RewriteStatepointsForGC pass...");

    let opt_result = std::process::Command::new("opt")
        .args(["-passes=rewrite-statepoints-for-gc", "dynamic_abstract.ll", "-S", "-o", "dynamic_lowered.ll"])
        .output();

    match opt_result {
        Ok(result) if result.status.success() => {
            let lowered = std::fs::read_to_string("dynamic_lowered.ll").unwrap();
            println!("\n─── Lowered IR (with statepoints) ───\n");

            // Show just the build_list function
            let mut in_func = false;
            for line in lowered.lines() {
                if line.contains("define") && line.contains("build_list") {
                    in_func = true;
                }
                if in_func {
                    println!("{}", line);
                    if line.starts_with("}") {
                        break;
                    }
                }
            }

            // Key observation
            println!("\n╔════════════════════════════════════════════════════════════╗");
            println!("║ KEY OBSERVATIONS:                                          ║");
            println!("╠════════════════════════════════════════════════════════════╣");
            println!("║ 1. Tagged values (i64) can contain either:                 ║");
            println!("║    - Immediate values (fixnums, chars) - NOT traced by GC  ║");
            println!("║    - Heap pointers - TRACED and RELOCATED by GC            ║");
            println!("║                                                            ║");
            println!("║ 2. The GC must check tag bits before tracing:              ║");
            println!("║    if (val & 0b111) == 0 && val != 0 => trace as pointer   ║");
            println!("║    otherwise => ignore (it's an immediate)                 ║");
            println!("║                                                            ║");
            println!("║ 3. LLVM statepoints mark call sites where GC may occur     ║");
            println!("║    After the call, gc.relocate reads the (possibly moved)  ║");
            println!("║    pointer from where LLVM saved it                        ║");
            println!("║                                                            ║");
            println!("║ 4. Stack maps tell the GC where live values are stored     ║");
            println!("║    The GC filters these by tag to find actual pointers     ║");
            println!("╚════════════════════════════════════════════════════════════╝");
        }
        Ok(result) => {
            println!("opt failed: {}", String::from_utf8_lossy(&result.stderr));
        }
        Err(e) => {
            println!("Failed to run opt: {}", e);
        }
    }
}
