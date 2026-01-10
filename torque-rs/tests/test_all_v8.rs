//! Test our Torque parser against ALL V8 Torque files

use std::fs;
use std::path::Path;

fn parse_file(path: &Path) -> Result<usize, String> {
    let source = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read file: {}", e))?;

    torque_rs::parse_source(&source)
        .map(|ast| ast.declarations.len())
        .map_err(|e| format!("{}", e))
}

#[test]
fn test_all_v8_torque_files() {
    let v8_dir = Path::new("../v8_source");
    if !v8_dir.exists() {
        println!("V8 source not found at ../v8_source - skipping test");
        return;
    }

    let mut successes = Vec::new();
    let mut failures = Vec::new();

    // Find all .tq files
    fn find_tq_files(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    find_tq_files(&path, files);
                } else if path.extension().map_or(false, |e| e == "tq") {
                    files.push(path);
                }
            }
        }
    }

    let mut all_files = Vec::new();
    find_tq_files(v8_dir, &mut all_files);
    all_files.sort();

    for path in &all_files {
        let filename = path.file_name().unwrap().to_str().unwrap().to_string();
        match parse_file(path) {
            Ok(decl_count) => successes.push((filename, decl_count)),
            Err(e) => {
                // Get first line of error
                let first_line = e.lines().next().unwrap_or(&e).to_string();
                failures.push((filename, first_line));
            }
        }
    }

    println!("\n=== ALL V8 Torque Files Parsing Results ===\n");

    let success_rate = (successes.len() as f64 / all_files.len() as f64) * 100.0;
    println!(
        "Overall: {}/{} files parsed ({:.1}%)\n",
        successes.len(),
        all_files.len(),
        success_rate
    );

    println!("Successes ({}):", successes.len());
    for (name, decl_count) in &successes {
        println!("  ✓ {} ({} declarations)", name, decl_count);
    }

    // Group failures by error type
    let mut error_categories: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for (name, err) in &failures {
        // Extract the key part of the error
        let category = if err.contains("Lex error") {
            "Lexer error"
        } else if err.contains("Operator") {
            "Missing: operator declarations"
        } else if err.contains("Constexpr") && err.contains("expected: {Some(LParen)}") {
            "Missing: if constexpr"
        } else if err.contains("enum") {
            "Missing: extern enum"
        } else if err.contains("Semicolon") && err.contains("extern") {
            "Missing: extern class without body"
        } else if err.contains("Generates") {
            "Missing: generates clause for extern class"
        } else if err.contains("Percent") {
            "Missing: % intrinsics"
        } else if err.contains("bitfield") {
            "Missing: bitfield struct"
        } else if err.contains("transitioning") {
            "Missing: transitioning keyword"
        } else {
            "Other parse errors"
        };
        error_categories
            .entry(category.to_string())
            .or_default()
            .push(name.clone());
    }

    println!("\nFailures by category:");
    let mut cats: Vec<_> = error_categories.iter().collect();
    cats.sort_by_key(|(_, files)| std::cmp::Reverse(files.len()));
    for (category, files) in cats {
        println!("  {} ({} files):", category, files.len());
        for f in files.iter().take(3) {
            println!("    - {}", f);
        }
        if files.len() > 3 {
            println!("    ... and {} more", files.len() - 3);
        }
    }
}
