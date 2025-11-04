/// Demonstrates memory-efficient modes for multi-GB heap dumps
///
/// Shows three modes:
/// 1. Normal mode - stores all instances (good for small dumps)
/// 2. Low-memory mode - only tracks counts, no instance data
/// 3. Selective mode - only stores instances of specific classes

use hprof_parser::{HeapExplorer, ExplorerConfig};
use std::fs::File;
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Multi-GB Heap Dump Support ===\n");

    let filename = "java-test/heap-dump.hprof";

    // Mode 1: Normal mode (stores everything)
    println!("Mode 1: Normal Mode (default)");
    println!("- Stores all instances in memory");
    println!("- Good for small dumps (<1GB)");
    println!("- Allows full instance inspection\n");

    let start = Instant::now();
    let file = File::open(filename)?;
    let mut explorer = HeapExplorer::new(file)?;
    explorer.process_all()?;
    let time = start.elapsed();

    let stats = explorer.get_statistics();
    println!("Results:");
    println!("{}", stats);
    println!("Time: {:.2}s\n", time.as_secs_f64());

    // Mode 2: Low-memory mode (no instance storage)
    println!("Mode 2: Low-Memory Mode");
    println!("- Does NOT store instance data");
    println!("- Only tracks class counts and metadata");
    println!("- Good for multi-GB dumps");
    println!("- Constant memory usage\n");

    let start = Instant::now();
    let file = File::open(filename)?;
    let config = ExplorerConfig::low_memory();
    let mut explorer = HeapExplorer::with_config(file, config)?;
    explorer.process_all()?;
    let time = start.elapsed();

    let stats = explorer.get_statistics();
    println!("Results:");
    println!("{}", stats);
    println!("Note: 0 instances stored, but counts are tracked!");

    // Show that counts still work
    let top = explorer.top_classes_by_count(5);
    println!("\nTop 5 classes by count (from streaming):");
    for (i, (name, count)) in top.iter().enumerate() {
        println!("  {}. {} - {} instances", i + 1, name, count);
    }
    println!("\nTime: {:.2}s", time.as_secs_f64());
    println!("Memory: Minimal (no instance data stored)\n");

    // Mode 3: Selective indexing (only Person instances)
    println!("Mode 3: Selective Indexing");
    println!("- Only stores instances matching a pattern");
    println!("- Pattern: 'Person'");
    println!("- Good for focused analysis on large dumps\n");

    let start = Instant::now();
    let file = File::open(filename)?;
    let config = ExplorerConfig::selective("Person", Some(10)); // Max 10 per class
    let mut explorer = HeapExplorer::with_config(file, config)?;
    explorer.process_all()?;
    let time = start.elapsed();

    let stats = explorer.get_statistics();
    println!("Results:");
    println!("{}", stats);

    // Show Person instances
    let person_classes = explorer.find_classes("Person");
    if let Some((class_id, class_name)) = person_classes.first() {
        let stored = explorer.get_instances_of_class(*class_id).len();
        let total = explorer.get_instance_count(*class_id);
        println!("\n{} instances:", class_name);
        println!("  Stored in memory: {} (limited to 10)", stored);
        println!("  Total in heap: {}", total);

        // Show field structure of stored instances
        if let Some(first) = explorer.get_instances_of_class(*class_id).first() {
            println!("\nField structure (from stored instance):");
            let fields = explorer.get_instance_fields(first);
            for (i, (name, ftype)) in fields.iter().enumerate() {
                println!("  [{}] {}: {:?}", i, name, ftype);
            }
        }
    }

    println!("\nTime: {:.2}s", time.as_secs_f64());
    println!("Memory: Reduced (only Person instances stored)\n");

    // Summary
    println!("=== Summary ===\n");
    println!("For multi-GB heap dumps, use:");
    println!();
    println!("1. Low-Memory Mode:");
    println!("   let config = ExplorerConfig::low_memory();");
    println!("   let explorer = HeapExplorer::with_config(file, config)?;");
    println!("   - Constant memory usage");
    println!("   - Can still get class counts");
    println!("   - Cannot inspect individual instances");
    println!();
    println!("2. Selective Indexing:");
    println!("   let config = ExplorerConfig::selective(\"MyClass\", Some(100));");
    println!("   let explorer = HeapExplorer::with_config(file, config)?;");
    println!("   - Only stores instances of matching classes");
    println!("   - Limits instances per class (e.g., 100 samples)");
    println!("   - Can inspect stored instances");
    println!();
    println!("3. Custom Configuration:");
    println!("   let config = ExplorerConfig {{");
    println!("       store_instances: false,");
    println!("       max_instances_per_class: Some(1000),");
    println!("       instance_filter: Some(\"java/util\".to_string()),");
    println!("       store_strings: true,");
    println!("   }};");

    Ok(())
}
