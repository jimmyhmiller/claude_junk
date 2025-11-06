use hprof_parser::{HeapExplorer, Result};

fn main() -> Result<()> {
    println!("=== GC Root Path Finding Example ===\n");

    // Load the heap dump
    let explorer = HeapExplorer::new("java-test/heap-dump.hprof")?;

    // Get a Person instance
    let (person_class_id, _) = explorer.find_class_exact("com/example/HeapDumpGenerator$Person")
        .expect("Person class not found");

    let instances = explorer.get_instances_of_class(person_class_id)?;
    let first_person = instances.first().expect("No Person instances found");

    println!("Finding GC root paths for Person object {:x}\n", first_person.object_id);

    // Find paths to GC roots (max 3 paths, max depth 15)
    let paths = explorer.find_gc_root_paths(first_person.object_id, 3, 15)?;

    if paths.is_empty() {
        println!("No paths to GC roots found.");
        println!("This object may be unreachable (eligible for GC).");
    } else {
        println!("Found {} path(s) to GC roots:\n", paths.len());

        for (i, path) in paths.iter().enumerate() {
            println!("Path {}:", i + 1);
            for (j, (obj_id, desc)) in path.iter().enumerate() {
                if j == 0 {
                    println!("  {:x} (target object)", obj_id);
                } else if j == path.len() - 1 {
                    println!("  └─> {:x} [GC ROOT]", obj_id);
                } else {
                    println!("  └─> {:x} via {}", obj_id, desc);
                }
            }
            println!();
        }
    }

    println!("\nExplanation:");
    println!("- The target object is kept alive because it's reachable from a GC root");
    println!("- GC roots are special objects that are always considered reachable:");
    println!("  - Thread stacks");
    println!("  - JNI global references");
    println!("  - Static fields");
    println!("  - Monitor objects");
    println!("- Objects only reachable from non-root objects may be garbage collected");

    Ok(())
}
