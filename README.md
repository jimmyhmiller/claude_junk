# HPROF Parser

A streaming Rust library for parsing Java HPROF heap dumps with constant memory usage. Designed to be LLM-friendly for exploring and analyzing large heap dumps.

## Features

- **Streaming Parser**: Process heap dumps without loading the entire file into memory
- **Constant Memory**: Designed to handle multi-gigabyte heap dumps efficiently
- **LLM-Friendly API**: High-level `HeapExplorer` API for easy heap exploration
- **Type-Safe**: Full type safety with Rust's type system
- **Comprehensive**: Supports all HPROF record types and heap dump sub-records

## Project Structure

```
.
├── src/
│   ├── lib.rs          # Main library entry point
│   ├── error.rs        # Error types
│   ├── types.rs        # Core data types
│   ├── record.rs       # Record definitions
│   ├── parser.rs       # Streaming parser
│   └── explorer.rs     # High-level exploration API
├── examples/
│   ├── basic_parser.rs     # Low-level streaming parser example
│   └── heap_explorer.rs    # High-level explorer example
├── java-test/          # Java project for generating test heap dumps
└── HPROF_FORMAT.md     # HPROF format specification
```

## Quick Start

### 1. Generate a Test Heap Dump

```bash
cd java-test
./compile-and-run.sh
```

This will create a `heap-dump.hprof` file with sample data.

### 2. Build the Rust Library

```bash
cargo build --release
```

### 3. Run Examples

**Basic streaming parser:**
```bash
cargo run --example basic_parser heap-dump.hprof
```

**High-level heap explorer:**
```bash
cargo run --example heap_explorer heap-dump.hprof
```

**Search for specific classes:**
```bash
cargo run --example heap_explorer heap-dump.hprof "String"
```

## Usage

### Low-Level Streaming API

For maximum control and minimal memory usage:

```rust
use hprof_parser::{HprofParser, Record};
use std::fs::File;

let file = File::open("heap.hprof")?;
let mut parser = HprofParser::new(file)?;

while let Some(record) = parser.next_record()? {
    match record {
        Record::String { id, text } => {
            println!("String: {}", text);
        }
        Record::LoadClass { class_name_id, .. } => {
            println!("Loaded class");
        }
        Record::InstanceDump { object_id, .. } => {
            println!("Instance: {:x}", object_id);
        }
        _ => {}
    }
}
```

### High-Level Explorer API

For convenient heap exploration (LLM-friendly):

```rust
use hprof_parser::HeapExplorer;
use std::fs::File;

let file = File::open("heap.hprof")?;
let mut explorer = HeapExplorer::new(file)?;

// Process the entire heap dump
explorer.process_all()?;

// Get statistics
println!("{}", explorer.get_statistics());

// Find classes by name
let classes = explorer.find_classes("ArrayList");

// Get top classes by instance count
let top = explorer.top_classes_by_count(10);
for (name, count) in top {
    println!("{}: {} instances", name, count);
}

// Get instances of a specific class
if let Some((class_id, _)) = classes.first() {
    let instances = explorer.get_instances_of_class(*class_id);
    println!("Found {} instances", instances.len());
}
```

## LLM-Driven Exploration

The `HeapExplorer` API is specifically designed to be driven by an LLM. Key features:

1. **Simple Queries**: Methods like `find_classes()`, `get_class_name()`, and `top_classes_by_count()`
2. **Incremental Processing**: Process the heap in chunks with `process_n()`
3. **Statistics**: Get overview with `get_statistics()`
4. **Search**: Find classes and instances by pattern matching

### Example LLM Prompts

- "Show me all classes in the heap"
- "Find classes related to 'HashMap'"
- "What are the top 10 classes by instance count?"
- "How many String objects are in the heap?"
- "List all GC roots"

## HPROF Format

See [HPROF_FORMAT.md](HPROF_FORMAT.md) for a detailed specification of the HPROF binary format.

Key concepts:
- **Header**: Version, ID size, timestamp
- **Records**: Top-level records with tags (String, LoadClass, HeapDump, etc.)
- **Heap Dump Sub-records**: GC roots, class dumps, instance dumps, array dumps
- **Streaming**: Records can be processed one at a time without loading the entire file

## Memory Usage

The library supports three modes for handling different heap dump sizes:

### 1. Normal Mode (Default)
- Stores all instances in memory for fast queries
- Good for dumps < 1GB
- Full instance inspection capabilities

```rust
let explorer = HeapExplorer::new(file)?;
```

### 2. Low-Memory Mode
- **Perfect for multi-GB dumps**
- Doesn't store instance data, only counts
- Constant memory usage regardless of dump size
- Can still query class counts and statistics

```rust
let config = ExplorerConfig::low_memory();
let explorer = HeapExplorer::with_config(file, config)?;

// Still works! Counts tracked during streaming
let top = explorer.top_classes_by_count(10);
```

### 3. Selective Indexing
- Only stores instances of specific classes
- Good for focused analysis on large dumps
- Configurable per-class instance limits

```rust
// Only store Person instances, max 100 per class
let config = ExplorerConfig::selective("Person", Some(100));
let explorer = HeapExplorer::with_config(file, config)?;
```

**Example:** Run `cargo run --release --example multi_gb_support` to see all three modes in action.

## Testing

The included Java test project (`java-test/`) creates a realistic heap dump with:
- Multiple classes (Person, Company, DataBuffer)
- 250+ object instances
- Collections (ArrayList, HashMap, HashSet)
- Large byte arrays (1MB total)
- 1000+ unique strings

## Future Enhancements

Potential improvements:
- External indexing for very large dumps
- Reference graph traversal
- Memory leak detection
- Path to GC root calculation
- Object retention analysis
- JSON export
- Interactive CLI

## License

MIT

## Contributing

Contributions welcome! This library is designed to be simple and focused on streaming parsing with constant memory.