/// Scan and count actual record types in the file

use hprof_parser::{HprofParser, Record};
use std::fs::File;
use std::collections::HashMap;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Scanning All Records ===\n");

    let file = File::open("java-test/heap-dump.hprof")?;
    let mut parser = HprofParser::new(file)?;

    let mut record_counts: HashMap<String, usize> = HashMap::new();
    let mut instance_class_counts: HashMap<u64, usize> = HashMap::new();
    let mut string_class_id: Option<u64> = None;

    // First, find String class ID
    while let Some(record) = parser.next_record()? {
        match &record {
            Record::LoadClass { class_name_id, object_id, .. } => {
                // We'll check this later
            }
            Record::String { id, text } => {
                if text == "java/lang/String" {
                    println!("Found 'java/lang/String' string with ID: {:x}", id);
                }
            }
            _ => {}
        }
    }

    // Second pass - count everything
    let file = File::open("java-test/heap-dump.hprof")?;
    let mut parser = HprofParser::new(file)?;
    let mut string_class_obj_id: Option<u64> = None;
    let mut class_name_to_id: HashMap<u64, u64> = HashMap::new();

    while let Some(record) = parser.next_record()? {
        let record_type = match &record {
            Record::String { .. } => "String",
            Record::LoadClass { class_name_id, object_id, .. } => {
                class_name_to_id.insert(*object_id, *class_name_id);
                "LoadClass"
            }
            Record::ClassDump(info) => {
                // Check if this is the String class
                if let Some(name_id) = class_name_to_id.get(&info.object_id) {
                    // We'd need to lookup the string, but let's just record it
                }
                "ClassDump"
            }
            Record::InstanceDump { class_object_id, .. } => {
                *instance_class_counts.entry(*class_object_id).or_insert(0) += 1;
                "InstanceDump"
            }
            Record::ObjectArrayDump { .. } => "ObjectArrayDump",
            Record::PrimitiveArrayDump { .. } => "PrimitiveArrayDump",
            Record::Root(_) => "Root",
            Record::Frame(_) => "Frame",
            Record::Trace(_) => "Trace",
            Record::StartThread { .. } => "StartThread",
            Record::EndThread { .. } => "EndThread",
            Record::HeapSummary { .. } => "HeapSummary",
            Record::HeapDumpEnd => "HeapDumpEnd",
            Record::UnloadClass { .. } => "UnloadClass",
            Record::Unknown { .. } => "Unknown",
        };

        *record_counts.entry(record_type.to_string()).or_insert(0) += 1;
    }

    println!("\n=== Record Type Counts ===");
    let mut sorted: Vec<_> = record_counts.iter().collect();
    sorted.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
    for (record_type, count) in sorted {
        println!("  {}: {}", record_type, count);
    }

    println!("\n=== Top 10 Classes by InstanceDump Count ===");
    let mut sorted_classes: Vec<_> = instance_class_counts.iter().collect();
    sorted_classes.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
    for (class_id, count) in sorted_classes.iter().take(10) {
        println!("  Class ID {:x}: {} instances", class_id, count);
    }

    println!("\n=== Looking for String class specifically ===");
    println!("Class ID 730882dc0 (from earlier debug): {} instances",
             instance_class_counts.get(&0x730882dc0).unwrap_or(&0));

    Ok(())
}
