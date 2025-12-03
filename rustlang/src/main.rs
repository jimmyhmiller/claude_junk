mod gc_runtime;
mod stackmap;

use inkwell::context::Context;
use inkwell::intrinsics::Intrinsic;
use inkwell::module::Linkage;
use inkwell::targets::{
    CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine,
};
use inkwell::AddressSpace;
use inkwell::OptimizationLevel;
use object::{Object, ObjectSection};
use std::path::Path;

/// Demonstrates LLVM statepoint-based GC with inkwell and a real GC runtime

fn main() {
    println!("=== LLVM Statepoints + Real GC Runtime Demo ===\n");

    // First, let's test the GC runtime standalone
    test_gc_runtime_standalone();

    // Then generate and test with LLVM statepoints
    test_with_statepoints();
}

/// Test the GC runtime without LLVM - proves the GC mechanics work
fn test_gc_runtime_standalone() {
    println!("\n========================================");
    println!("Part 1: Testing GC Runtime (standalone)");
    println!("========================================\n");

    // Initialize GC
    gc_runtime::gc_init();

    // Simulate what LLVM-generated code would do:
    // Allocate some objects and keep them as "roots" on our stack

    // Allocate obj1
    let obj1: *mut u8 = gc_runtime::gc_alloc(64);
    println!("Allocated obj1 at {:p}", obj1);

    // Write some data to obj1
    unsafe {
        *(obj1 as *mut u64) = 0xDEAD_BEEF_CAFE_BABE;
    }

    // Allocate obj2 (simulates a safepoint call)
    let obj2: *mut u8 = gc_runtime::gc_alloc(64);
    println!("Allocated obj2 at {:p}", obj2);

    unsafe {
        *(obj2 as *mut u64) = 0x1234_5678_9ABC_DEF0;
    }

    // Allocate obj3
    let obj3: *mut u8 = gc_runtime::gc_alloc(128);
    println!("Allocated obj3 at {:p}", obj3);

    // Now trigger a GC!
    // In real statepoint code, the runtime would find roots via stack maps.
    // Here, we'll manually pass them.

    println!("\n--- Triggering GC ---");
    println!("Before GC:");
    println!("  obj1 = {:p} (data: {:#x})", obj1, unsafe {
        *(obj1 as *const u64)
    });
    println!("  obj2 = {:p} (data: {:#x})", obj2, unsafe {
        *(obj2 as *const u64)
    });
    println!("  obj3 = {:p}", obj3);

    // Create mutable root slots (this is what the stack would look like)
    let mut root1 = obj1;
    let mut root2 = obj2;
    let mut root3 = obj3;

    // Collect garbage with our roots
    let mut roots: [*mut *mut u8; 3] = [
        &mut root1 as *mut *mut u8,
        &mut root2 as *mut *mut u8,
        &mut root3 as *mut *mut u8,
    ];

    unsafe {
        if let Some(gc) = &mut gc_runtime::GC {
            gc.collect(&mut roots.iter().map(|r| *r).collect::<Vec<_>>());
        }
    }

    println!("\nAfter GC:");
    println!("  root1 = {:p} (was {:p})", root1, obj1);
    println!("  root2 = {:p} (was {:p})", root2, obj2);
    println!("  root3 = {:p} (was {:p})", root3, obj3);

    // Verify the data was preserved!
    println!("\nVerifying data integrity:");
    let data1 = unsafe { *(root1 as *const u64) };
    let data2 = unsafe { *(root2 as *const u64) };
    println!("  root1 data: {:#x} (expected: 0xDEAD_BEEF_CAFE_BABE)", data1);
    println!("  root2 data: {:#x} (expected: 0x1234_5678_9ABC_DEF0)", data2);

    if data1 == 0xDEAD_BEEF_CAFE_BABE && data2 == 0x1234_5678_9ABC_DEF0 {
        println!("\n✓ SUCCESS: Data survived GC and relocation!");
        if root1 != obj1 {
            println!("✓ Objects were actually MOVED (root1: {:p} -> {:p})", obj1, root1);
        }
    } else {
        println!("\n✗ FAILURE: Data corruption after GC!");
    }

    gc_runtime::gc_stats();
}

/// Test with actual LLVM statepoints
fn test_with_statepoints() {
    println!("\n\n========================================");
    println!("Part 2: LLVM Statepoints Integration");
    println!("========================================\n");

    // Generate the IR
    let (module_ir, object_path) = generate_statepoint_code();

    // Parse the stack maps from the object file
    if let Some(path) = object_path {
        parse_and_show_stackmaps(&path);
    }
}

fn generate_statepoint_code() -> (String, Option<String>) {
    println!("Generating IR with statepoints...\n");

    let context = Context::create();
    let module = context.create_module("gc_test");
    let builder = context.create_builder();

    let i64_type = context.i64_type();
    let gc_ptr_type = context.ptr_type(AddressSpace::from(1));

    // Declare external GC functions
    let alloc_fn_type = gc_ptr_type.fn_type(&[i64_type.into()], false);
    let alloc_fn = module.add_function("gc_alloc", alloc_fn_type, Some(Linkage::External));

    // Create a test function
    let fn_type = i64_type.fn_type(&[], false);
    let function = module.add_function("gc_test_function", fn_type, None);
    function.set_gc("statepoint-example");

    let entry = context.append_basic_block(function, "entry");
    builder.position_at_end(entry);

    // Allocate and use objects
    let size = i64_type.const_int(64, false);

    // obj1 = gc_alloc(64)
    let obj1 = builder
        .build_call(alloc_fn, &[size.into()], "obj1")
        .unwrap()
        .try_as_basic_value()
        .left()
        .unwrap()
        .into_pointer_value();

    // Store a value in obj1
    let magic = i64_type.const_int(0xCAFEBABE, false);
    builder.build_store(obj1, magic).unwrap();

    // obj2 = gc_alloc(64) -- this is a safepoint, obj1 must be relocated
    let _obj2 = builder.build_call(alloc_fn, &[size.into()], "obj2").unwrap();

    // Load from obj1 (after potential GC - the pass will fix this)
    let val = builder.build_load(i64_type, obj1, "val").unwrap();
    builder.build_return(Some(&val)).unwrap();

    let ir = module.print_to_string().to_string();
    println!("Abstract IR (before statepoint pass):\n{}\n", ir);

    // Write to file
    module.print_to_file("gc_test_abstract.ll").unwrap();

    // Run RewriteStatepointsForGC pass
    println!("Running RewriteStatepointsForGC pass...\n");

    let opt_result = std::process::Command::new("opt")
        .args([
            "-passes=rewrite-statepoints-for-gc",
            "gc_test_abstract.ll",
            "-S",
            "-o",
            "gc_test_lowered.ll",
        ])
        .output();

    match opt_result {
        Ok(result) if result.status.success() => {
            let lowered = std::fs::read_to_string("gc_test_lowered.ll").unwrap();
            println!("Lowered IR (with statepoints):\n{}\n", lowered);

            // Compile to object file
            println!("Compiling to object file...\n");

            let llc_result = std::process::Command::new("llc")
                .args([
                    "-filetype=obj",
                    "-relocation-model=pic",
                    "gc_test_lowered.ll",
                    "-o",
                    "gc_test.o",
                ])
                .output();

            match llc_result {
                Ok(r) if r.status.success() => {
                    println!("Generated gc_test.o\n");
                    return (ir, Some("gc_test.o".to_string()));
                }
                Ok(r) => {
                    println!("llc failed: {}", String::from_utf8_lossy(&r.stderr));
                }
                Err(e) => {
                    println!("Failed to run llc: {}", e);
                }
            }
        }
        Ok(result) => {
            println!("opt failed: {}", String::from_utf8_lossy(&result.stderr));
        }
        Err(e) => {
            println!("Failed to run opt: {}", e);
        }
    }

    (ir, None)
}

fn parse_and_show_stackmaps(object_path: &str) {
    println!("========================================");
    println!("Parsing Stack Maps from {}", object_path);
    println!("========================================\n");

    let data = std::fs::read(object_path).expect("Failed to read object file");
    let obj = object::File::parse(&*data).expect("Failed to parse object file");

    // Find the .llvm_stackmaps section
    let stackmap_section = obj.sections().find(|s| {
        s.name()
            .map(|n| n == ".llvm_stackmaps")
            .unwrap_or(false)
    });

    match stackmap_section {
        Some(section) => {
            let section_data = section.data().expect("Failed to read section data");
            println!(
                "Found .llvm_stackmaps section: {} bytes\n",
                section_data.len()
            );

            match stackmap::StackMap::parse(section_data) {
                Ok(stackmap) => {
                    println!("Stack Map Version: {}", stackmap.header.version);
                    println!("Functions: {}", stackmap.header.num_functions);
                    println!("Records: {}", stackmap.header.num_records);
                    println!();

                    for (i, func) in stackmap.functions.iter().enumerate() {
                        println!(
                            "Function {}: addr={:#x}, stack_size={}, records={}",
                            i, func.address, func.stack_size, func.record_count
                        );
                    }
                    println!();

                    for (i, record) in stackmap.records.iter().enumerate() {
                        println!(
                            "Record {}: id={}, offset={}, {} locations",
                            i,
                            record.id,
                            record.instruction_offset,
                            record.locations.len()
                        );

                        let gc_locs = stackmap.get_gc_locations(record);
                        if !gc_locs.is_empty() {
                            println!("  GC pointer locations:");
                            for (base, derived) in &gc_locs {
                                println!("    base: {}, derived: {}", base, derived);
                            }
                        }
                    }

                    println!("\n✓ Stack maps parsed successfully!");
                    println!("  These tell the GC exactly where to find live pointers");
                    println!("  at each safepoint (call instruction).\n");
                }
                Err(e) => {
                    println!("Failed to parse stack map: {}", e);
                }
            }
        }
        None => {
            println!("No .llvm_stackmaps section found!");
        }
    }

    // Show raw llvm-readobj output too
    println!("\n--- llvm-readobj --stackmap output ---\n");
    let readobj = std::process::Command::new("llvm-readobj")
        .args(["--stackmap", object_path])
        .output();

    if let Ok(result) = readobj {
        if result.status.success() {
            println!("{}", String::from_utf8_lossy(&result.stdout));
        }
    }
}
