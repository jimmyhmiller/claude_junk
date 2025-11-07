use hprof_parser::{HeapExplorer, Result, FieldValue};
use std::env;
use std::io::{self, Write};

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <heap-dump.hprof>", args[0]);
        eprintln!();
        eprintln!("Interactive HPROF heap dump explorer");
        std::process::exit(1);
    }

    let file_path = &args[1];

    println!("Loading heap dump: {}", file_path);
    let explorer = HeapExplorer::new(file_path)?;
    println!("✓ Loaded\n");

    let stats = explorer.get_statistics();
    println!("{}", stats);
    println!();

    // Interactive REPL
    loop {
        print!("> ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();

        if input.is_empty() {
            continue;
        }

        if input == "quit" || input == "exit" {
            break;
        }

        let parts: Vec<&str> = input.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        match parts[0] {
            "help" => print_help(),
            "classes" => {
                if parts.len() < 2 {
                    println!("Usage: classes <pattern>");
                } else {
                    list_classes(&explorer, parts[1]);
                }
            }
            "top" => {
                let n = if parts.len() > 1 {
                    parts[1].parse().unwrap_or(10)
                } else {
                    10
                };
                show_top_classes(&explorer, n);
            }
            "instances" => {
                if parts.len() < 2 {
                    println!("Usage: instances <class-name>");
                } else {
                    if let Err(e) = show_instances(&explorer, parts[1]) {
                        println!("Error: {}", e);
                    }
                }
            }
            "count" => {
                if parts.len() < 2 {
                    println!("Usage: count <class-name>");
                } else {
                    show_count(&explorer, parts[1]);
                }
            }
            "roots" => {
                if parts.len() < 2 {
                    println!("Usage: roots <object-id-hex>");
                } else {
                    if let Err(e) = show_gc_roots(&explorer, parts[1]) {
                        println!("Error: {}", e);
                    }
                }
            }
            "inspect" => {
                if parts.len() < 2 {
                    println!("Usage: inspect <object-id-hex>");
                } else {
                    if let Err(e) = inspect_instance(&explorer, parts[1]) {
                        println!("Error: {}", e);
                    }
                }
            }
            "extract" => {
                if parts.len() < 3 {
                    println!("Usage: extract <object-id-hex> <field-index>");
                } else {
                    if let Err(e) = extract_field(&explorer, parts[1], parts[2]) {
                        println!("Error: {}", e);
                    }
                }
            }
            "stats" => {
                let stats = explorer.get_statistics();
                println!("{}", stats);
            }
            _ => {
                println!("Unknown command: {}", parts[0]);
                println!("Type 'help' for available commands");
            }
        }
    }

    Ok(())
}

fn print_help() {
    println!("Available commands:");
    println!("  help                          - Show this help");
    println!("  stats                         - Show heap statistics");
    println!("  classes <pattern>             - Find classes matching pattern");
    println!("  top [n]                       - Show top N classes by instance count (default 10)");
    println!("  count <class-name>            - Show instance count for exact class name");
    println!("  instances <class-name>        - List all instances of exact class name");
    println!("  inspect <object-id-hex>       - Show fields of an instance");
    println!("  extract <object-id-hex> <idx> - Extract field value at index");
    println!("  roots <object-id-hex>         - Show GC root paths for an object");
    println!("  quit, exit                    - Exit the program");
}

fn list_classes(explorer: &HeapExplorer, pattern: &str) {
    let classes = explorer.find_classes(pattern);
    println!("Found {} classes matching '{}':", classes.len(), pattern);
    for (id, name) in classes.iter().take(50) {
        let count = explorer.get_instance_count(*id);
        println!("  {} - {} instances (id: {:x})", name, count, id);
    }
    if classes.len() > 50 {
        println!("  ... and {} more", classes.len() - 50);
    }
}

fn show_top_classes(explorer: &HeapExplorer, n: usize) {
    let top = explorer.top_classes_by_count(n);
    println!("Top {} classes by instance count:", n);
    for (i, (name, count)) in top.iter().enumerate() {
        println!("  {}. {} - {} instances", i + 1, name, count);
    }
}

fn show_count(explorer: &HeapExplorer, class_name: &str) {
    if let Some((class_id, name)) = explorer.find_class_exact(class_name) {
        let count = explorer.get_instance_count(class_id);
        println!("{}: {} instances", name, count);
    } else {
        println!("Class not found: {}", class_name);
        println!("Try: classes {}", class_name);
    }
}

fn show_instances(explorer: &HeapExplorer, class_name: &str) -> Result<()> {
    if let Some((class_id, name)) = explorer.find_class_exact(class_name) {
        let count = explorer.get_instance_count(class_id);
        println!("Scanning for {} instances of {}...", count, name);

        let instances = explorer.get_instances_of_class(class_id)?;
        println!("Found {} instances:", instances.len());

        for (i, inst) in instances.iter().take(20).enumerate() {
            println!("  [{}] object_id: {:x}, size: {} bytes",
                     i, inst.object_id, inst.data.len());
        }

        if instances.len() > 20 {
            println!("  ... and {} more", instances.len() - 20);
        }
    } else {
        println!("Class not found: {}", class_name);
        println!("Try: classes {}", class_name);
    }

    Ok(())
}

fn show_gc_roots(explorer: &HeapExplorer, object_id_str: &str) -> Result<()> {
    // Parse hex object ID
    let object_id = u64::from_str_radix(object_id_str, 16)
        .map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "Invalid hex object ID")
        })?;

    println!("Finding GC root paths for object {:x}...", object_id);
    println!("(max 5 paths, max depth 20)\n");

    let paths = explorer.find_gc_root_paths(object_id, 5, 20)?;

    if paths.is_empty() {
        println!("No paths to GC roots found.");
        println!("This object may be unreachable (eligible for garbage collection).");
    } else {
        println!("Found {} path(s) to GC roots:\n", paths.len());

        for (i, path) in paths.iter().enumerate() {
            println!("Path {}:", i + 1);
            for (j, (obj_id, desc)) in path.iter().enumerate() {
                if j == 0 {
                    println!("  {:x} (target)", obj_id);
                } else if j == path.len() - 1 {
                    println!("  └─> {:x} [GC ROOT]", obj_id);
                } else {
                    println!("  └─> {:x} via {}", obj_id, desc);
                }
            }
            println!();
        }
    }

    Ok(())
}

fn inspect_instance(explorer: &HeapExplorer, object_id_str: &str) -> Result<()> {
    // Parse hex object ID
    let object_id = u64::from_str_radix(object_id_str, 16)
        .map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "Invalid hex object ID")
        })?;

    // Get the instance
    if let Some(instance) = explorer.get_instance(object_id)? {
        // Get class name
        let class_name = explorer.get_class_name(instance.class_object_id)
            .unwrap_or("Unknown");

        println!("Instance {:x}", object_id);
        println!("Class: {}", class_name);
        println!("Data size: {} bytes", instance.data.len());
        println!("\nFields:");

        // Get and display fields
        let fields = explorer.get_instance_fields(&instance);
        for (idx, (name, field_type)) in fields.iter().enumerate() {
            print!("  [{}] {}: {:?}", idx, name, field_type);

            // Try to extract the value
            if let Ok(value) = explorer.get_field_value(&instance, idx) {
                match value {
                    FieldValue::Object(0) => println!(" = null"),
                    FieldValue::Object(id) => {
                        println!(" = {:x}", id);
                        // If it's a String, try to extract the value
                        if let Ok(Some(obj)) = explorer.get_instance(id) {
                            if let Ok(Some(s)) = explorer.extract_string_value(&obj) {
                                let preview = if s.len() > 50 {
                                    format!("{}...", &s[..50])
                                } else {
                                    s
                                };
                                println!("      → \"{}\"", preview);
                            }
                        }
                    }
                    FieldValue::Int(v) => println!(" = {}", v),
                    FieldValue::Long(v) => println!(" = {}", v),
                    FieldValue::Boolean(v) => println!(" = {}", v),
                    FieldValue::Byte(v) => println!(" = {}", v),
                    FieldValue::Short(v) => println!(" = {}", v),
                    FieldValue::Char(v) => println!(" = '{}'", char::from_u32(v as u32).unwrap_or('?')),
                    FieldValue::Float(v) => println!(" = {}", v),
                    FieldValue::Double(v) => println!(" = {}", v),
                }
            } else {
                println!();
            }
        }
    } else {
        println!("Object {:x} not found", object_id);
    }

    Ok(())
}

fn extract_field(explorer: &HeapExplorer, object_id_str: &str, field_idx_str: &str) -> Result<()> {
    // Parse hex object ID
    let object_id = u64::from_str_radix(object_id_str, 16)
        .map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "Invalid hex object ID")
        })?;

    // Parse field index
    let field_idx: usize = field_idx_str.parse()
        .map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "Invalid field index")
        })?;

    // Get the instance
    if let Some(instance) = explorer.get_instance(object_id)? {
        let class_name = explorer.get_class_name(instance.class_object_id)
            .unwrap_or("Unknown");

        println!("Instance: {:x} ({})", object_id, class_name);

        // Get field info
        let fields = explorer.get_instance_fields(&instance);
        if field_idx >= fields.len() {
            println!("Error: Field index {} out of bounds (max {})", field_idx, fields.len() - 1);
            return Ok(());
        }

        let (field_name, field_type) = &fields[field_idx];
        println!("Field [{}]: {} ({:?})", field_idx, field_name, field_type);

        // Extract the value
        if let Ok(value) = explorer.get_field_value(&instance, field_idx) {
            match value {
                FieldValue::Object(0) => println!("Value: null"),
                FieldValue::Object(id) => {
                    println!("Value: {:x} (Object reference)", id);

                    // Try to get more info about this object
                    if let Ok(Some(obj)) = explorer.get_instance(id) {
                        if let Some(obj_class) = explorer.get_class_name(obj.class_object_id) {
                            println!("  Class: {}", obj_class);

                            // If it's a String, extract the value
                            if let Ok(Some(s)) = explorer.extract_string_value(&obj) {
                                println!("  String value: \"{}\"", s);
                            }
                        }
                    }
                }
                FieldValue::Int(v) => println!("Value: {}", v),
                FieldValue::Long(v) => println!("Value: {}", v),
                FieldValue::Boolean(v) => println!("Value: {}", v),
                FieldValue::Byte(v) => println!("Value: {}", v),
                FieldValue::Short(v) => println!("Value: {}", v),
                FieldValue::Char(v) => println!("Value: '{}'", char::from_u32(v as u32).unwrap_or('?')),
                FieldValue::Float(v) => println!("Value: {}", v),
                FieldValue::Double(v) => println!("Value: {}", v),
            }
        } else {
            println!("Error: Could not extract field value");
        }
    } else {
        println!("Object {:x} not found", object_id);
    }

    Ok(())
}
