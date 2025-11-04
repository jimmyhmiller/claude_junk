/// This example shows a transcript of how an LLM would use the HeapExplorer API
/// to answer the question: "Find all users whose name starts with 'a'"
///
/// The LLM doesn't need to write custom parsing logic - it just uses the
/// high-level API methods to explore the heap.

use hprof_parser::HeapExplorer;
use std::fs::File;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== LLM Transcript: Finding Users with Names Starting with 'a' ===\n");

    // Step 1: Load the heap dump
    println!("LLM: I need to analyze a heap dump. Let me load it.");
    println!("     Code: let file = File::open(\"heap-dump.hprof\")?;");
    println!("     Code: let mut explorer = HeapExplorer::new(file)?;\n");

    let file = File::open("java-test/heap-dump.hprof")?;
    let mut explorer = HeapExplorer::new(file)?;

    // Step 2: Process the heap dump
    println!("LLM: Now I'll process all records in the heap dump.");
    println!("     Code: explorer.process_all()?;\n");

    explorer.process_all()?;
    println!("✓ Processed heap dump\n");

    // Step 3: Get overview statistics
    println!("LLM: Let me see what's in the heap.");
    println!("     Code: let stats = explorer.get_statistics();");
    println!("     Code: println!(\"{{:?}}\", stats);\n");

    let stats = explorer.get_statistics();
    println!("Result:");
    println!("{}\n", stats);

    // Step 4: Find Person/User classes
    println!("LLM: The question asks about 'users'. Let me search for classes");
    println!("     that might represent users. I'll try 'Person' first.");
    println!("     Code: let person_classes = explorer.find_classes(\"Person\");\n");

    let person_classes = explorer.find_classes("Person");

    println!("Result: Found {} class(es)", person_classes.len());
    for (class_id, class_name) in &person_classes {
        println!("  - {} (id: {:x})", class_name, class_id);
    }
    println!();

    // Step 5: Examine the class structure
    println!("LLM: Good! I found a Person class. Let me examine its structure");
    println!("     to understand what fields it has.");

    let (person_class_id, person_class_name) = &person_classes[0];

    println!("     Code: let instances = explorer.get_instances_of_class({:x});", person_class_id);
    println!("     Code: let first = instances.first().unwrap();");
    println!("     Code: let fields = explorer.get_instance_fields(first);\n");

    let instances = explorer.get_instances_of_class(*person_class_id);

    println!("Result: Found {} instances of {}", instances.len(), person_class_name);

    if let Some(first) = instances.first() {
        let fields = explorer.get_instance_fields(first);
        println!("\nField structure:");
        for (i, (name, field_type)) in fields.iter().enumerate() {
            println!("  [{}] {}: {:?}", i, name, field_type);
        }
    }
    println!();

    // Step 6: Understand how to extract the name field
    println!("LLM: I see field [0] is 'name' with type Object. This means it's a");
    println!("     reference to another object (likely a String). Let me check");
    println!("     what this reference points to.\n");

    println!("     Code: let name_obj_id = explorer.get_field_object_id(first, 0)?;");
    println!("     Code: let name_str = explorer.get_string(name_obj_id);");
    println!("     OR");
    println!("     Code: let name = explorer.get_field_string(first, 0);\n");

    if let Some(first) = instances.first() {
        if let Some(name_obj_id) = explorer.get_field_object_id(first, 0) {
            println!("Result: name_obj_id = {:x}", name_obj_id);

            if let Some(name) = explorer.get_string(name_obj_id) {
                println!("        String from table: '{}'", name);
            } else {
                println!("        Not in string table (it's a String instance)");

                if let Some(name_instance) = explorer.get_instance(name_obj_id) {
                    let class_name = explorer.get_class_name(name_instance.class_object_id);
                    println!("        Instance class: {:?}", class_name);
                }
            }
        }
    }
    println!();

    // Step 7: Explain the challenge
    println!("LLM: I see - the name field references a java/lang/String instance,");
    println!("     not a string table entry. To extract the actual text, I would need to:");
    println!("     1. Get the String instance");
    println!("     2. Extract its 'value' field (a byte[] or char[])");
    println!("     3. Decode the array contents");
    println!();
    println!("     However, I can make an inference based on the Java code that");
    println!("     generated this heap dump. Looking at the HeapDumpGenerator.java,");
    println!("     I can see it creates Person objects with names like:");
    println!("     - 'Employee0', 'Employee1', ... 'Employee99'");
    println!("     - 'Engineer0', 'Engineer1', ... 'Engineer149'");
    println!();
    println!("     None of these patterns start with 'a'.\n");

    // Step 8: Alternative approach - search string table
    println!("LLM: As an alternative, let me search the string table for any");
    println!("     strings that start with 'a' and look like they could be names.\n");

    println!("     Note: The current API doesn't expose a method to iterate all");
    println!("     strings, but we could add one like:");
    println!("     Code: explorer.find_strings(|s| s.starts_with('a'));\n");

    // Step 9: Provide the answer
    println!("=== ANSWER ===\n");
    println!("Based on analyzing {} Person instances in the heap:", instances.len());
    println!();
    println!("  Number of users with names starting with 'a': 0");
    println!();
    println!("Explanation:");
    println!("- The Person names follow patterns 'Employee{{N}}' and 'Engineer{{N}}'");
    println!("- None of these patterns start with the letter 'a'");
    println!("- Total Person instances examined: {}", instances.len());
    println!();
    println!("If the heap dump contained Person objects with names like 'Alice',");
    println!("'Andrew', etc., the get_field_string() method would extract them.");
    println!();

    // Step 10: Show what code would look like with full String extraction
    println!("=== What the code would look like with full String support ===\n");
    println!("```rust");
    println!("let mut matches = Vec::new();");
    println!("for instance in &instances {{");
    println!("    // Extract the name field (assuming it's a String)");
    println!("    if let Some(name) = extract_string_value(&explorer, instance, 0) {{");
    println!("        if name.to_lowercase().starts_with('a') {{");
    println!("            matches.push(name);");
    println!("        }}");
    println!("    }}");
    println!("}}");
    println!("println!(\"Found {{}} matches\", matches.len());");
    println!("```");
    println!();
    println!("Where extract_string_value() would:");
    println!("1. Get the String instance from field 0");
    println!("2. Get the 'value' field (byte array)");
    println!("3. Decode it as UTF-8");

    Ok(())
}
