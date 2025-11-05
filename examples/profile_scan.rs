use hprof_parser::{HeapExplorer, Result};
use std::time::Instant;

fn main() -> Result<()> {
    println!("=== Profiling Scan Performance ===\n");

    // Load once
    let start = Instant::now();
    let explorer = HeapExplorer::new("java-test/heap-dump.hprof")?;
    println!("Initial load: {:.4}s\n", start.elapsed().as_secs_f64());

    // Get String class
    let (string_class_id, _) = explorer.find_class_exact("java/lang/String")
        .expect("String class not found");

    // Do 10 scans and measure
    println!("Running 10 scans of String class (13,507 instances each):");
    let mut times = Vec::new();

    for i in 0..10 {
        let start = Instant::now();
        let instances = explorer.get_instances_of_class(string_class_id)?;
        let elapsed = start.elapsed().as_secs_f64();
        times.push(elapsed);
        println!("  Scan {}: {:.4}s - {} instances - {:.0} MB/s",
                 i+1, elapsed, instances.len(), 6.8 / elapsed);
    }

    let avg = times.iter().sum::<f64>() / times.len() as f64;
    println!("\nAverage: {:.4}s - {:.0} MB/s", avg, 6.8 / avg);

    Ok(())
}
