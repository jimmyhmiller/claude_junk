mod gc_runtime;
mod stackmap;
mod tagged_value;
mod shadow_stack;
mod tagged_gc;
mod dynamic_demo;
mod jit_demo;

use inkwell::context::Context;
use inkwell::module::Linkage;
use inkwell::AddressSpace;
use object::{Object, ObjectSection};

fn main() {
    // Check command line for which demo to run
    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 {
        match args[1].as_str() {
            "dynamic" => {
                dynamic_demo::run_demo();
                return;
            }
            "jit" => {
                jit_demo::run_demo();
                return;
            }
            "all" => {
                println!("=== LLVM Statepoints + GC Movement Demo ===\n");
                demonstrate_gc_movement();
                demonstrate_statepoints();

                println!("\n\n");
                dynamic_demo::run_demo();

                println!("\n\n");
                jit_demo::run_demo();
                return;
            }
            _ => {}
        }
    }

    println!("=== LLVM Statepoints + GC Movement Demo ===\n");

    // The clearest demonstration of GC movement
    demonstrate_gc_movement();

    // Then show LLVM statepoint integration
    demonstrate_statepoints();

    println!("\n");
    println!("Available demos:");
    println!("  cargo run           # Basic GC movement demo");
    println!("  cargo run dynamic   # Tagged pointer demo (educational)");
    println!("  cargo run jit       # ** WORKING JIT with shadow stack GC **");
    println!("  cargo run all       # Run all demos");
}

/// Crystal clear demonstration that GC actually moves objects
fn demonstrate_gc_movement() {
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║     DEMONSTRATING THAT GC ACTUALLY MOVES OBJECTS             ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    // Initialize GC
    gc_runtime::gc_init();

    // Allocate an object
    println!("Step 1: Allocate an object and store data");
    println!("─────────────────────────────────────────\n");

    let obj_ptr: *mut u8 = gc_runtime::gc_alloc(64);
    let original_addr = obj_ptr as usize;

    // Store distinctive pattern
    unsafe {
        let ptr = obj_ptr as *mut u64;
        *ptr.offset(0) = 0xAAAA_AAAA_AAAA_AAAA;
        *ptr.offset(1) = 0xBBBB_BBBB_BBBB_BBBB;
        *ptr.offset(2) = 0xCCCC_CCCC_CCCC_CCCC;
        *ptr.offset(3) = 0xDDDD_DDDD_DDDD_DDDD;
    }

    println!("  Original address: {:#x}", original_addr);
    println!("  Memory contents at original address:");
    dump_memory(obj_ptr, 32);

    // Save the pointer in a "root slot" (simulating stack)
    println!("\nStep 2: Save pointer to root slot (simulating stack variable)");
    println!("──────────────────────────────────────────────────────────────\n");

    let mut root_slot: *mut u8 = obj_ptr;
    println!("  root_slot contains: {:#x}", root_slot as usize);

    // Trigger GC
    println!("\nStep 3: Trigger garbage collection");
    println!("───────────────────────────────────\n");

    let roots: &mut [*mut *mut u8] = &mut [&mut root_slot as *mut *mut u8];

    unsafe {
        if let Some(gc) = &mut gc_runtime::GC {
            gc.collect(roots);
        }
    }

    let new_addr = root_slot as usize;

    // Show the results
    println!("\nStep 4: Examine results");
    println!("───────────────────────\n");

    println!("  Original address: {:#x}", original_addr);
    println!("  New address:      {:#x}", new_addr);
    println!("  Difference:       {:#x} bytes",
        if new_addr > original_addr { new_addr - original_addr } else { original_addr - new_addr });

    if original_addr != new_addr {
        println!("\n  ✓ OBJECT WAS MOVED TO A DIFFERENT ADDRESS!");
    } else {
        println!("\n  ✗ Object was NOT moved (same address)");
    }

    println!("\n  Memory at NEW address (should have our data):");
    dump_memory(root_slot, 32);

    println!("\n  Memory at OLD address (should be garbage/0xDD):");
    dump_memory(original_addr as *mut u8, 32);

    // Verify data integrity
    println!("\nStep 5: Verify data survived the move");
    println!("──────────────────────────────────────\n");

    unsafe {
        let ptr = root_slot as *mut u64;
        let v0 = *ptr.offset(0);
        let v1 = *ptr.offset(1);
        let v2 = *ptr.offset(2);
        let v3 = *ptr.offset(3);

        println!("  Reading from NEW address {:#x}:", new_addr);
        println!("    [0]: {:#018x} (expected 0xAAAAAAAAAAAAAAAA)", v0);
        println!("    [1]: {:#018x} (expected 0xBBBBBBBBBBBBBBBB)", v1);
        println!("    [2]: {:#018x} (expected 0xCCCCCCCCCCCCCCCC)", v2);
        println!("    [3]: {:#018x} (expected 0xDDDDDDDDDDDDDDDD)", v3);

        if v0 == 0xAAAA_AAAA_AAAA_AAAA
            && v1 == 0xBBBB_BBBB_BBBB_BBBB
            && v2 == 0xCCCC_CCCC_CCCC_CCCC
            && v3 == 0xDDDD_DDDD_DDDD_DDDD
        {
            println!("\n  ✓✓✓ ALL DATA INTACT AFTER MOVE! ✓✓✓");
        } else {
            println!("\n  ✗ Data corrupted!");
        }
    }

    // Show what happens if you use the OLD pointer
    println!("\n  What if we read from OLD address {:#x}? (DON'T DO THIS!):", original_addr);
    unsafe {
        let old_ptr = original_addr as *mut u64;
        let v0 = *old_ptr.offset(0);
        println!("    [0]: {:#018x} <- GARBAGE! (we poisoned with 0xDD)", v0);
    }

    // Multiple GC cycles to really prove movement
    println!("\n\nStep 6: Multiple GC cycles (ping-pong between spaces)");
    println!("──────────────────────────────────────────────────────\n");

    for i in 1..=3 {
        let before = root_slot as usize;
        unsafe {
            if let Some(gc) = &mut gc_runtime::GC {
                gc.collect(roots);
            }
        }
        let after = root_slot as usize;
        println!("  GC #{}: {:#x} -> {:#x}", i + 1, before, after);
    }

    // Verify data still intact after all those moves
    unsafe {
        let ptr = root_slot as *mut u64;
        let v0 = *ptr.offset(0);
        if v0 == 0xAAAA_AAAA_AAAA_AAAA {
            println!("\n  ✓ Data still intact after {} GC cycles!", 4);
        }
    }

    gc_runtime::gc_stats();
}

fn dump_memory(ptr: *mut u8, len: usize) {
    print!("    ");
    for i in 0..len {
        unsafe {
            print!("{:02x} ", *ptr.add(i));
        }
        if (i + 1) % 16 == 0 && i + 1 < len {
            print!("\n    ");
        }
    }
    println!();
}

/// Show LLVM statepoints and how they track pointers
fn demonstrate_statepoints() {
    println!("\n\n╔══════════════════════════════════════════════════════════════╗");
    println!("║     LLVM STATEPOINT INTEGRATION                              ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    let context = Context::create();
    let module = context.create_module("gc_test");
    let builder = context.create_builder();

    let i64_type = context.i64_type();
    let gc_ptr_type = context.ptr_type(AddressSpace::from(1));

    // Declare gc_alloc
    let alloc_fn_type = gc_ptr_type.fn_type(&[i64_type.into()], false);
    let alloc_fn = module.add_function("gc_alloc", alloc_fn_type, Some(Linkage::External));

    // Create test function
    let fn_type = i64_type.fn_type(&[], false);
    let function = module.add_function("test_function", fn_type, None);
    function.set_gc("statepoint-example");

    let entry = context.append_basic_block(function, "entry");
    builder.position_at_end(entry);

    let size = i64_type.const_int(64, false);

    // obj1 = gc_alloc(64)
    let obj1 = builder
        .build_call(alloc_fn, &[size.into()], "obj1")
        .unwrap()
        .try_as_basic_value()
        .left()
        .unwrap()
        .into_pointer_value();

    // store to obj1
    let val = i64_type.const_int(0x12345678, false);
    builder.build_store(obj1, val).unwrap();

    // obj2 = gc_alloc(64) -- SAFEPOINT! obj1 might move here
    let _obj2 = builder.build_call(alloc_fn, &[size.into()], "obj2").unwrap();

    // load from obj1 -- needs relocated pointer!
    let loaded = builder.build_load(i64_type, obj1, "loaded").unwrap();
    builder.build_return(Some(&loaded)).unwrap();

    let ir = module.print_to_string().to_string();
    println!("Abstract IR (before statepoints):\n");
    println!("{}", ir);

    module.print_to_file("gc_test_abstract.ll").unwrap();

    // Run statepoint pass
    println!("\nRunning RewriteStatepointsForGC...\n");

    let opt_result = std::process::Command::new("opt")
        .args(["-passes=rewrite-statepoints-for-gc", "gc_test_abstract.ll", "-S", "-o", "gc_test_lowered.ll"])
        .output()
        .expect("Failed to run opt");

    if opt_result.status.success() {
        let lowered = std::fs::read_to_string("gc_test_lowered.ll").unwrap();
        println!("Lowered IR (with statepoints):\n");
        println!("{}", lowered);

        // Compile to object
        let llc_result = std::process::Command::new("llc")
            .args(["-filetype=obj", "-relocation-model=pic", "gc_test_lowered.ll", "-o", "gc_test.o"])
            .output()
            .expect("Failed to run llc");

        if llc_result.status.success() {
            // Parse and show stack maps
            show_stackmaps("gc_test.o");

            // Show assembly
            show_assembly();
        }
    }

    // Simulate execution with GC
    println!("\n\n╔══════════════════════════════════════════════════════════════╗");
    println!("║     SIMULATING EXECUTION WITH GC                             ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    simulate_statepoint_execution();
}

fn show_stackmaps(path: &str) {
    println!("\n─── Stack Maps ───\n");

    let data = std::fs::read(path).expect("Failed to read object");
    let obj = object::File::parse(&*data).expect("Failed to parse object");

    if let Some(section) = obj.sections().find(|s| s.name().map(|n| n == ".llvm_stackmaps").unwrap_or(false)) {
        let section_data = section.data().expect("Failed to read section");

        if let Ok(stackmap) = stackmap::StackMap::parse(section_data) {
            for (i, record) in stackmap.records.iter().enumerate() {
                println!("Safepoint {} at instruction offset {}:", i, record.instruction_offset);
                let gc_locs = stackmap.get_gc_locations(record);
                if gc_locs.is_empty() {
                    println!("  No live GC pointers");
                } else {
                    for (base, derived) in &gc_locs {
                        println!("  Live pointer: {} (base: {})", derived, base);
                    }
                }
            }
        }
    }
}

fn show_assembly() {
    println!("\n─── Generated Assembly ───\n");

    let asm = std::process::Command::new("llc")
        .args(["gc_test_lowered.ll", "-o", "-"])
        .output();

    if let Ok(result) = asm {
        if result.status.success() {
            let asm_str = String::from_utf8_lossy(&result.stdout);
            let mut in_func = false;
            for line in asm_str.lines() {
                if line.contains("test_function:") {
                    in_func = true;
                }
                if in_func {
                    println!("{}", line);
                    if line.starts_with(".Lfunc_end") {
                        break;
                    }
                }
            }
        }
    }
}

fn simulate_statepoint_execution() {
    println!("This simulates what the generated code does:\n");

    // Reinit GC
    gc_runtime::gc_init();

    // Step 1: Allocate obj1
    println!("1. obj1 = gc_alloc(64)");
    let obj1 = gc_runtime::gc_alloc(64);
    let obj1_original = obj1 as usize;
    println!("   obj1 = {:#x}", obj1_original);

    // Store value
    println!("2. *obj1 = 0x12345678");
    unsafe { *(obj1 as *mut u64) = 0x12345678; }

    // Step 2: Save obj1 to stack (statepoint does this)
    println!("3. [statepoint] Save obj1 to stack slot before calling gc_alloc");
    let mut stack_slot = obj1;
    println!("   stack_slot @ {:p} = {:#x}", &stack_slot as *const _, stack_slot as usize);

    // Step 3: Call gc_alloc (with GC happening inside)
    println!("4. obj2 = gc_alloc(64)  [GC HAPPENS HERE!]");

    // Trigger GC before the allocation returns
    let roots: &mut [*mut *mut u8] = &mut [&mut stack_slot as *mut *mut u8];
    unsafe {
        if let Some(gc) = &mut gc_runtime::GC {
            gc.collect(roots);
        }
    }

    let _obj2 = gc_runtime::gc_alloc(64);

    // Step 4: gc.relocate - read from stack slot
    println!("5. [gc.relocate] Read relocated pointer from stack slot");
    let obj1_relocated = stack_slot;
    let obj1_new = obj1_relocated as usize;
    println!("   obj1 was {:#x}, now {:#x}", obj1_original, obj1_new);

    if obj1_original != obj1_new {
        println!("   ✓ POINTER WAS UPDATED BY GC!");
    }

    // Step 5: Load from relocated pointer
    println!("6. Load value from relocated obj1");
    let loaded = unsafe { *(obj1_relocated as *const u64) };
    println!("   *obj1 = {:#x}", loaded);

    if loaded == 0x12345678 {
        println!("\n✓✓✓ SUCCESS: Read correct value from MOVED object! ✓✓✓");
    } else {
        println!("\n✗ FAILURE: Data corrupted!");
    }

    gc_runtime::gc_stats();
}
