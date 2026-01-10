//! Test our Torque parser against real V8 Torque files

use std::fs;
use std::path::Path;

fn parse_file(path: &Path) -> Result<(), String> {
    let source = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read file: {}", e))?;

    torque_rs::parse_source(&source)
        .map(|_| ())
        .map_err(|e| format!("{}", e))
}

fn parse_snippet(source: &str) -> Result<(), String> {
    torque_rs::parse_source(source)
        .map(|_| ())
        .map_err(|e| format!("{}", e))
}

fn test_v8_files(files: &[&str]) -> (Vec<String>, Vec<(String, String)>) {
    let mut successes = Vec::new();
    let mut failures = Vec::new();

    for file_path in files {
        let path = Path::new(file_path);
        if !path.exists() {
            failures.push((file_path.to_string(), "File not found".to_string()));
            continue;
        }
        let filename = path.file_name().unwrap().to_str().unwrap().to_string();

        match parse_file(path) {
            Ok(()) => successes.push(filename),
            Err(e) => failures.push((filename, e)),
        }
    }

    (successes, failures)
}

#[test]
fn test_v8_torque_files() {
    // Test against various V8 torque files (simpler ones first)
    let test_files = [
        "../v8_source/src/objects/allocation-site.tq",
        "../v8_source/src/objects/cell.tq",
        "../v8_source/src/objects/bigint.tq",
        "../v8_source/src/objects/hole.tq",
        "../v8_source/test/torque/test-torque.tq",
        "../v8_source/src/objects/js-array.tq",
    ];

    let (successes, failures) = test_v8_files(&test_files);

    println!("\n=== V8 Torque File Parsing Results ===\n");
    println!("Successes ({}):", successes.len());
    for s in &successes {
        println!("  ✓ {}", s);
    }

    println!("\nFailures ({}):", failures.len());
    for (name, err) in &failures {
        println!("  ✗ {}:", name);
        // Print first few lines of error
        for line in err.lines().take(5) {
            println!("      {}", line);
        }
    }

    println!("\n{}/{} files parsed successfully", successes.len(), test_files.len());
}

#[test]
fn test_missing_features() {
    // Test individual V8 Torque features we may not support yet
    let features = [
        (
            "extern class without body",
            "extern class Foo extends HeapObject;",
        ),
        (
            "extern class with body",
            "extern class Foo extends HeapObject { value: Smi; }",
        ),
        (
            "operator declaration",
            "operator '.value' macro LoadValue(cell: Cell): Object { return Null; }",
        ),
        (
            "operator with assignment",
            "operator '.value=' macro StoreValue(cell: Cell, value: Object): void { }",
        ),
        (
            "extern enum",
            "extern enum ElementsKind extends uint31 { PACKED_SMI_ELEMENTS, HOLEY_SMI_ELEMENTS }",
        ),
        (
            "if constexpr",
            "macro Test(): bool { if constexpr (true) { return true; } else { return false; } }",
        ),
        (
            "percent intrinsic",
            "macro Test(): Smi { return %RawDownCast<Smi>(0); }",
        ),
        (
            "bitfield struct",
            "bitfield struct Flags extends uint8 { a: bool: 1 bit; }",
        ),
        (
            "transitioning keyword",
            "transitioning macro Foo(): void { }",
        ),
        (
            "array field with length",
            "class Foo extends HeapObject { entries[count]: Smi; }",
        ),
        (
            "catch block",
            "macro Test(): void { try { } catch (_e, _msg) { } }",
        ),
        (
            "new expression",
            "macro Test(): Foo { return new Foo{a: 1}; }",
        ),
        (
            "reference type",
            "macro Test(x: &Smi): void { }",
        ),
        (
            "dereference operator",
            "macro Test(x: &Smi): Smi { return *x; }",
        ),
        (
            "address-of operator",
            "macro Test(x: Foo): &Smi { return &x.value; }",
        ),
        (
            "spread in struct literal",
            "macro Test(): FixedArray { return new FixedArray{...iter}; }",
        ),
        (
            "static_assert",
            "macro Test(): void { static_assert(1 == 1); }",
        ),
        (
            "@ annotations with complex args",
            "@incrementUseCounter('v8::Isolate::kFoo') builtin Test(): Smi { return 0; }",
        ),
        (
            "const field in struct",
            "struct Foo { const x: int32; }",
        ),
        (
            "macro method in struct",
            "struct Foo { macro GetX(): int32 { return this.x; } x: int32; }",
        ),
        (
            "generic specialization",
            "LoadElement<FixedArray>(a: JSArray): JSAny { return Null; }",
        ),
    ];

    println!("\n=== Feature Support Test ===\n");

    let mut supported = Vec::new();
    let mut unsupported = Vec::new();

    for (name, snippet) in features {
        match parse_snippet(snippet) {
            Ok(()) => supported.push(name),
            Err(e) => unsupported.push((name, e)),
        }
    }

    println!("Supported ({}/{}):", supported.len(), features.len());
    for name in &supported {
        println!("  ✓ {}", name);
    }

    println!("\nUnsupported ({}/{}):", unsupported.len(), features.len());
    for (name, err) in &unsupported {
        // Just print first line of error
        let first_line = err.lines().next().unwrap_or(&err);
        println!("  ✗ {}: {}", name, first_line);
    }

    println!("\nFeature support: {}/{}", supported.len(), features.len());
}
