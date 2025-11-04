use hprof_parser::{HprofParser, Record};
use std::fs::File;
use std::io::BufReader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <heap-dump.hprof>", args[0]);
        std::process::exit(1);
    }

    let filename = &args[1];
    println!("Opening heap dump: {}", filename);

    let file = File::open(filename)?;
    let mut parser = HprofParser::new(file)?;

    println!("Header: {:?}", parser.header());
    println!("\nProcessing records...\n");

    let mut record_counts = std::collections::HashMap::new();
    let mut record_num = 0;

    while let Some(record) = parser.next_record()? {
        record_num += 1;

        let record_type = match &record {
            Record::String { .. } => "String",
            Record::LoadClass { .. } => "LoadClass",
            Record::UnloadClass { .. } => "UnloadClass",
            Record::Frame(_) => "Frame",
            Record::Trace(_) => "Trace",
            Record::StartThread { .. } => "StartThread",
            Record::EndThread { .. } => "EndThread",
            Record::HeapSummary { .. } => "HeapSummary",
            Record::Root(_) => "Root",
            Record::ClassDump(_) => "ClassDump",
            Record::InstanceDump { .. } => "InstanceDump",
            Record::ObjectArrayDump { .. } => "ObjectArrayDump",
            Record::PrimitiveArrayDump { .. } => "PrimitiveArrayDump",
            Record::HeapDumpEnd => "HeapDumpEnd",
            Record::Unknown { .. } => "Unknown",
        };

        *record_counts.entry(record_type).or_insert(0) += 1;

        // Print first few records of each type
        if *record_counts.get(record_type).unwrap() <= 3 {
            match &record {
                Record::String { id, text } => {
                    let preview = if text.len() > 50 {
                        format!("{}...", &text[..50])
                    } else {
                        text.clone()
                    };
                    println!("[{}] String: id={:x}, text=\"{}\"", record_num, id, preview);
                }
                Record::LoadClass {
                    class_serial,
                    object_id,
                    ..
                } => {
                    println!(
                        "[{}] LoadClass: serial={}, id={:x}",
                        record_num, class_serial, object_id
                    );
                }
                Record::ClassDump(info) => {
                    println!(
                        "[{}] ClassDump: id={:x}, instance_size={}, {} static fields, {} instance fields",
                        record_num,
                        info.object_id,
                        info.instance_size,
                        info.static_fields.len(),
                        info.instance_fields.len()
                    );
                }
                Record::InstanceDump {
                    object_id,
                    class_object_id,
                    data,
                    ..
                } => {
                    println!(
                        "[{}] InstanceDump: id={:x}, class_id={:x}, {} bytes",
                        record_num,
                        object_id,
                        class_object_id,
                        data.len()
                    );
                }
                _ => {}
            }
        }

        if record_num % 1000 == 0 {
            println!("[{}] Processed {} records...", record_num, record_num);
        }
    }

    println!("\n=== Summary ===");
    println!("Total records: {}", record_num);
    println!("\nRecord counts:");
    let mut counts: Vec<_> = record_counts.iter().collect();
    counts.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    for (record_type, count) in counts {
        println!("  {}: {}", record_type, count);
    }

    Ok(())
}
