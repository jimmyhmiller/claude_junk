use hprof_parser::{HeapExplorer, Result, FieldValue};

fn main() -> Result<()> {
    println!("=== Field Value Extraction Example ===\n");

    let explorer = HeapExplorer::new("java-test/heap-dump.hprof")?;

    // Example 1: Extract fields from Person objects
    println!("1. Person Field Extraction");
    println!("=========================");

    let (person_class_id, _) = explorer.find_class_exact("com/example/HeapDumpGenerator$Person")
        .expect("Person class not found");

    let instances = explorer.get_instances_of_class(person_class_id)?;
    println!("Found {} Person instances\n", instances.len());

    // Show fields for first 5 instances
    for (i, inst) in instances.iter().take(5).enumerate() {
        println!("Person instance {} (object_id: {:x}):", i, inst.object_id);

        // Field 0: name (java/lang/String reference)
        if let Ok(FieldValue::Object(name_id)) = explorer.get_field_value(inst, 0) {
            println!("  name: {:x}", name_id);

            // Try to extract the String value
            if let Ok(Some(name_inst)) = explorer.get_instance(name_id) {
                if let Ok(Some(name_str)) = explorer.extract_string_value(&name_inst) {
                    println!("    → \"{}\"", name_str);
                }
            }
        }

        // Field 1: age (int)
        if let Ok(FieldValue::Int(age)) = explorer.get_field_value(inst, 1) {
            println!("  age: {}", age);
        }

        // Field 2: company (object reference)
        if let Ok(FieldValue::Object(company_id)) = explorer.get_field_value(inst, 2) {
            println!("  company: {:x}", company_id);
        }

        println!();
    }

    // Example 2: String deduplication analysis
    println!("\n2. String Deduplication Analysis");
    println!("================================");

    let (string_class_id, _) = explorer.find_class_exact("java/lang/String")
        .expect("String class not found");

    let string_instances = explorer.get_instances_of_class(string_class_id)?;
    println!("Total String instances: {}", string_instances.len());
    println!("Extracting first 20 String values...\n");

    let mut unique_strings = std::collections::HashMap::new();

    for (i, inst) in string_instances.iter().take(100).enumerate() {
        if let Ok(Some(value)) = explorer.extract_string_value(inst) {
            *unique_strings.entry(value.clone()).or_insert(0) += 1;

            if i < 20 {
                let preview = if value.len() > 50 {
                    format!("{}...", &value[..50])
                } else {
                    value.clone()
                };
                println!("  [{}] \"{}\"", i, preview);
            }
        }
    }

    println!("\nString Deduplication Statistics (first 100 strings):");
    println!("  Total examined: 100");
    println!("  Unique values: {}", unique_strings.len());

    // Show most common duplicates
    let mut sorted: Vec<_> = unique_strings.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(a.1));

    println!("\nTop 5 most duplicated strings:");
    for (value, count) in sorted.iter().take(5) {
        if **count > 1 {
            let preview = if value.len() > 40 {
                format!("{}...", &value[..40])
            } else {
                value.to_string()
            };
            println!("  {} occurrences: \"{}\"", count, preview);
        }
    }

    Ok(())
}
