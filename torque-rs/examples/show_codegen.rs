//! Example: Generate Java ASM code from Torque source
//!
//! This demonstrates how to use the JavaAsmBackend to generate
//! Java source code that uses ASM to emit bytecode.

use torque_rs::{parse_source, CodeGenerator};
use torque_rs::codegen::java_asm::JavaAsmBackend;

fn main() {
    let source = r#"
        // Example Torque code for Array.push builtin
        namespace array {
            // Type declarations
            type JSArray extends HeapObject generates 'TNode<JSArray>';
            type Smi extends Number generates 'TNode<Smi>';

            // A simple macro for adding two integers
            macro SmiAdd(a: Smi, b: Smi): Smi {
                return a + b;
            }

            // JavaScript builtin for Array.prototype.push
            javascript builtin ArrayPush(
                context: Context,
                receiver: JSArray,
                value: Object
            ): Number {
                let length: Smi = receiver.length;
                if (length == 0) {
                    return 0;
                }
                return length + 1;
            }

            // A more complex macro with control flow
            macro GetElement(array: JSArray, index: Smi): Object
                labels NotFound {
                if (index >= array.length) {
                    goto NotFound;
                }
                return array.elements[index];
            }
        }
    "#;

    println!("=== Torque Source ===");
    println!("{}", source);
    println!();

    // Parse the Torque source
    let ast = match parse_source(source) {
        Ok(ast) => ast,
        Err(e) => {
            eprintln!("Parse error: {}", e);
            return;
        }
    };

    println!("=== Parsed {} declarations ===", ast.declarations.len());
    println!();

    // Create the Java ASM backend
    let backend = JavaAsmBackend::new("js.builtins");
    let mut codegen = CodeGenerator::new(backend);

    // Generate code
    let output = match codegen.generate(&ast) {
        Ok(output) => output,
        Err(e) => {
            eprintln!("Code generation error: {}", e);
            return;
        }
    };

    // Print generated Java classes
    println!("=== Generated Java Classes ===");
    for (class_name, java_source) in &output.classes {
        println!("\n--- {} ---", class_name);
        println!("{}", java_source);
    }
}
