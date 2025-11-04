use hprof_parser::HeapExplorer;
use std::fs::File;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <heap-dump.hprof>", args[0]);
        std::process::exit(1);
    }

    let filename = &args[1];
    println!("Opening heap dump: {}", filename);

    let file = File::open(filename)?;
    let mut explorer = HeapExplorer::new(file)?;

    println!("Header: {:?}\n", explorer.header());

    println!("Processing heap dump (this may take a moment)...");
    explorer.process_all()?;

    println!("\n=== Heap Statistics ===");
    println!("{}\n", explorer.get_statistics());

    println!("=== Top 20 Classes by Instance Count ===");
    let top_classes = explorer.top_classes_by_count(20);
    for (i, (class_name, count)) in top_classes.iter().enumerate() {
        println!("{}. {} - {} instances", i + 1, class_name, count);
    }

    println!("\n=== All Loaded Classes ===");
    let classes = explorer.list_classes();
    println!("Total: {} classes\n", classes.len());

    // Group by package
    let mut packages: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for (_, class_name) in &classes {
        let package = if let Some(pos) = class_name.rfind('/') {
            class_name[..pos].to_string()
        } else {
            "(default)".to_string()
        };
        packages
            .entry(package)
            .or_insert_with(Vec::new)
            .push(class_name.clone());
    }

    let mut package_list: Vec<_> = packages.iter().collect();
    package_list.sort_by_key(|(_, classes)| std::cmp::Reverse(classes.len()));

    println!("Top packages:");
    for (package, classes) in package_list.iter().take(10) {
        println!("  {} - {} classes", package, classes.len());
    }

    // Search for specific classes if provided
    if args.len() > 2 {
        let search_term = &args[2];
        println!("\n=== Searching for classes matching '{}' ===", search_term);
        let matches = explorer.find_classes(search_term);
        for (class_id, class_name) in matches {
            let instances = explorer.get_instances_of_class(class_id);
            println!("  {} - {} instances", class_name, instances.len());
        }
    }

    println!("\n=== GC Roots ===");
    let roots = explorer.get_roots();
    println!("Total: {} roots\n", roots.len());

    let mut root_types: std::collections::HashMap<&str, usize> =
        std::collections::HashMap::new();
    for root in roots {
        let root_type = match root {
            hprof_parser::RootType::Unknown { .. } => "Unknown",
            hprof_parser::RootType::JniGlobal { .. } => "JNI Global",
            hprof_parser::RootType::JniLocal { .. } => "JNI Local",
            hprof_parser::RootType::JavaFrame { .. } => "Java Frame",
            hprof_parser::RootType::NativeStack { .. } => "Native Stack",
            hprof_parser::RootType::StickyClass { .. } => "Sticky Class",
            hprof_parser::RootType::ThreadBlock { .. } => "Thread Block",
            hprof_parser::RootType::MonitorUsed { .. } => "Monitor Used",
            hprof_parser::RootType::ThreadObject { .. } => "Thread Object",
        };
        *root_types.entry(root_type).or_insert(0) += 1;
    }

    println!("Root types:");
    let mut root_type_list: Vec<_> = root_types.iter().collect();
    root_type_list.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    for (root_type, count) in root_type_list {
        println!("  {}: {}", root_type, count);
    }

    println!("\nDone!");

    Ok(())
}
