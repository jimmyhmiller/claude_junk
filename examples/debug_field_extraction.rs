use hprof_parser::{HeapExplorer, Result, FieldValue};

fn main() -> Result<()> {
    println!("=== Debug Field Extraction ===\n");

    let explorer = HeapExplorer::new("java-test/heap-dump.hprof")?;

    // Get a Person instance
    let (person_class_id, _) = explorer.find_class_exact("com/example/HeapDumpGenerator$Person")
        .expect("Person class not found");

    let instances = explorer.get_instances_of_class(person_class_id)?;
    let first_person = instances.first().unwrap();

    println!("Person instance: {:x}", first_person.object_id);
    println!("Class: {:x}", first_person.class_object_id);
    println!("Data size: {} bytes\n", first_person.data.len());

    // Extract name field (field 0)
    println!("Extracting field 0 (name)...");
    if let Ok(FieldValue::Object(name_id)) = explorer.get_field_value(first_person, 0) {
        println!("  Name object ID: {:x}", name_id);

        // Try to get this instance
        match explorer.get_instance(name_id) {
            Ok(Some(name_inst)) => {
                println!("  Found name instance!");
                println!("    Class ID: {:x}", name_inst.class_object_id);

                if let Some(class_name) = explorer.get_class_name(name_inst.class_object_id) {
                    println!("    Class name: {}", class_name);
                }

                println!("    Data size: {} bytes", name_inst.data.len());

                // Try to extract string value
                println!("\n  Attempting string extraction...");
                match explorer.extract_string_value(&name_inst) {
                    Ok(Some(s)) => println!("    SUCCESS: \"{}\"", s),
                    Ok(None) => println!("    Returned None (not a String or extraction failed)"),
                    Err(e) => println!("    ERROR: {}", e),
                }

                // Show the fields of the String instance
                let fields = explorer.get_instance_fields(&name_inst);
                println!("\n  String instance fields:");
                for (i, (name, field_type)) in fields.iter().enumerate() {
                    println!("    [{}] {}: {:?}", i, name, field_type);

                    // Try to extract field 0 (value array)
                    if i == 0 {
                        if let Ok(field_val) = explorer.get_field_value(&name_inst, i) {
                            println!("        Value: {:?}", field_val);
                        }
                    }
                }
            }
            Ok(None) => println!("  Name instance not found!"),
            Err(e) => println!("  Error getting instance: {}", e),
        }
    }

    Ok(())
}
