/// Debug why String instances aren't found

use hprof_parser::HeapExplorer;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Debugging String Instance Issue ===\n");

    let explorer = HeapExplorer::new("java-test/heap-dump.hprof")?;

    // Check: What classes exist?
    println!("1. All classes containing 'String':");
    let string_classes = explorer.find_classes("String");
    for (class_id, class_name) in &string_classes {
        let count = explorer.get_instance_count(*class_id);
        println!("   {} (id: {:x}) - {} instances", class_name, class_id, count);
    }
    println!();

    // Check: Try to get instances
    println!("2. Try to get java/lang/String instances:");
    if let Some((class_id, class_name)) = explorer.find_class_exact("java/lang/String") {
        println!("   Class: {}", class_name);
        println!("   ID: {:x}", class_id);
        println!("   Expected count: {}", explorer.get_instance_count(class_id));

        println!("\n   Scanning file...");
        let instances = explorer.get_instances_of_class(class_id)?;
        println!("   Found: {} instances", instances.len());

        if instances.is_empty() {
            println!("\n   ❌ Problem: Count says {} but scan found 0",
                     explorer.get_instance_count(class_id));
        }
    }

    // Check: Try Person (we know this works)
    println!("\n3. Try Person instances (known working):");
    if let Some((class_id, class_name)) = explorer.find_class_exact("com/example/HeapDumpGenerator$Person") {
        println!("   Class: {}", class_name);
        println!("   ID: {:x}", class_id);
        println!("   Expected count: {}", explorer.get_instance_count(class_id));

        let instances = explorer.get_instances_of_class(class_id)?;
        println!("   Found: {} instances", instances.len());

        if instances.len() > 0 {
            println!("   ✓ This works!");
        }
    }

    // Check: What are the top classes by actual count?
    println!("\n4. Top 5 classes by count:");
    let top = explorer.top_classes_by_count(5);
    for (i, (name, count)) in top.iter().enumerate() {
        println!("   {}. {} - {} instances", i + 1, name, count);

        // Try to get instances for first one
        if i == 0 {
            if let Some((class_id, _)) = explorer.find_class_exact(name) {
                let instances = explorer.get_instances_of_class(class_id)?;
                println!("      → Scan found: {} instances", instances.len());
                if instances.len() != *count {
                    println!("      ❌ MISMATCH!");
                }
            }
        }
    }

    Ok(())
}
