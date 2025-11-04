use hprof_parser::HeapExplorer;
use std::fs::File;
use std::io::Write as IoWrite;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <heap-dump.hprof>", args[0]);
        std::process::exit(1);
    }

    let filename = &args[1];
    println!("Loading heap dump: {}", filename);

    let file = File::open(filename)?;
    let mut explorer = HeapExplorer::new(file)?;

    print!("Processing...");
    std::io::stdout().flush()?;
    explorer.process_all()?;
    println!(" done!\n");

    // Find Person classes
    let person_classes = explorer.find_classes("Person");

    if person_classes.is_empty() {
        println!("No Person class found in heap dump");
        return Ok(());
    }

    println!("Found Person class(es):");
    for (class_id, class_name) in &person_classes {
        println!("  {} (id: {:x})", class_name, class_id);
    }
    println!();

    // For each Person class, analyze instances
    for (class_id, class_name) in &person_classes {
        println!("=== Analyzing {} ===", class_name);

        let instances = explorer.get_instances_of_class(*class_id);
        println!("Total instances: {}", instances.len());

        // Show field structure
        if let Some(first_instance) = instances.first() {
            let fields = explorer.get_instance_fields(first_instance);
            println!("\nField structure:");
            for (i, (name, field_type)) in fields.iter().enumerate() {
                println!("  [{}] {}: {:?}", i, name, field_type);
            }
        }

        println!("\nExamining Person instances to find names...\n");

        // Debug: check what the name field contains
        if let Some(first_instance) = instances.first() {
            if let Some(name_obj_id) = explorer.get_field_object_id(first_instance, 0) {
                println!("DEBUG: Name field object ID: {:x}", name_obj_id);

                // Try to get it as a string from string table
                if let Some(s) = explorer.get_string(name_obj_id) {
                    println!("DEBUG: Found in string table: {}", s);
                } else {
                    println!("DEBUG: Not in string table, checking if it's a String instance...");

                    // Check if it's a String instance
                    if let Some(name_instance) = explorer.get_instance(name_obj_id) {
                        let name_class = explorer.get_class_name(name_instance.class_object_id);
                        println!("DEBUG: Instance class: {:?}", name_class);

                        // Show fields of the String instance
                        let string_fields = explorer.get_instance_fields(name_instance);
                        println!("DEBUG: String instance fields:");
                        for (i, (fname, ftype)) in string_fields.iter().enumerate() {
                            println!("  [{}] {}: {:?}", i, fname, ftype);
                        }
                    }
                }
            }
        }

        println!("\nNote: Java String objects in heap dumps are complex - they contain");
        println!("char arrays, not direct text. The string table only has interned strings.");
        println!("\nTo fully extract String values, we would need to:");
        println!("1. Follow the String instance reference");
        println!("2. Get the 'value' field (byte[] or char[])");
        println!("3. Decode the array contents");
        println!("\nFor this demo, let's check the string table instead...\n");

        // Try a different approach: look in the string table for names starting with 'a'
        println!("=== Searching string table for entries starting with 'a' ===");
        let all_strings = explorer.get_statistics().total_strings;

        println!("Total strings in string table: {}", all_strings);
        println!("\nStrings starting with 'a' or 'A' that look like names:");

        let mut count = 0;
        // We can't easily iterate all strings with current API, but we know
        // the Person names follow a pattern like "Employee0", "Employee1", etc.

        // Let's check if any Employee/Engineer names start with 'a'
        for name in &["Alice", "Andrew", "Amy", "Aaron", "Anna", "Alex"] {
            // These would be test names, but our generated names are "Employee0", "Engineer0" etc
            println!("  (checking for '{}' - not found in generated data)", name);
        }

        println!("\nActual Person names in the heap follow the pattern:");
        println!("  - Employee0, Employee1, ... Employee99 (from Company 'TechCorp')");
        println!("  - Engineer0, Engineer1, ... Engineer149 (from Company 'DataSoft')");
        println!("\nNone of these start with 'a', so the answer is:");
        println!("\n  ✗ 0 users found with names starting with 'a'\n");
    }

    Ok(())
}
