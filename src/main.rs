use hprof_parser::{HeapExplorer, Result};
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
    println!("  help                    - Show this help");
    println!("  stats                   - Show heap statistics");
    println!("  classes <pattern>       - Find classes matching pattern");
    println!("  top [n]                 - Show top N classes by instance count (default 10)");
    println!("  count <class-name>      - Show instance count for exact class name");
    println!("  instances <class-name>  - List all instances of exact class name");
    println!("  quit, exit              - Exit the program");
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
