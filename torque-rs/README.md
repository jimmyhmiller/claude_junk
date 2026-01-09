# torque-rs

A Rust parser and code generator for V8's Torque language, targeting the JVM.

## Overview

This project parses Torque (`.tq`) files and generates Java source code that uses the ASM library to emit bytecode at runtime. This enables implementing JavaScript built-in objects (Array, Object, etc.) using Torque's declarative syntax.

## Pipeline

```
Torque Source (.tq)
       ↓
   Parser (Rust)
       ↓
      AST
       ↓
   CodeGenerator
       ↓
 Java Source (ASM calls)
       ↓
    javac
       ↓
  .class files
       ↓
 JVM Runtime → Bytecode generation via ASM
```

## Building

```bash
cargo build
cargo test
```

## Testing

### Current Test Coverage

The project includes comprehensive testing at multiple levels:

1. **Parser Tests** (`src/parser.rs`): 20+ tests verifying Torque syntax parsing
2. **Codegen Integration Tests** (`tests/codegen_integration.rs`): 16 tests verifying correct ASM instruction generation
3. **Bytecode Verification** (`src/codegen/verify.rs`): JVM stack machine simulator that validates bytecode semantics

### Running Tests

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run specific test
cargo test test_simple_add_generates_iadd
```

### End-to-End Java Testing

To run full end-to-end tests (compile generated Java, run it, verify output), you need the ASM library:

```bash
# Option 1: Maven
mvn dependency:get -Dartifact=org.ow2.asm:asm:9.7

# Option 2: Gradle
# Add to build.gradle: implementation 'org.ow2.asm:asm:9.7'

# Option 3: Direct download
curl -O https://repo1.maven.org/maven2/org/ow2/asm/asm/9.7/asm-9.7.jar

# Option 4: apt (Debian/Ubuntu)
sudo apt-get install libasm-java
# JAR will be at /usr/share/java/asm.jar
```

Then compile and run the generated Java:

```bash
# Generate Java from Torque
cargo run --example show_codegen > Generated.java

# Compile with ASM
javac -cp asm-9.7.jar Generated.java

# Run
java -cp asm-9.7.jar:. Generated
```

## Example

```rust
use torque_rs::{parse_source, CodeGenerator};
use torque_rs::codegen::java_asm::JavaAsmBackend;

let torque = r#"
    namespace array {
        macro Add(a: int32, b: int32): int32 {
            return a + b;
        }
    }
"#;

let ast = parse_source(torque).unwrap();
let backend = JavaAsmBackend::new("js.builtins");
let mut codegen = CodeGenerator::new(backend);
let output = codegen.generate(&ast).unwrap();

for (class_name, java_source) in &output.classes {
    println!("// {}.java\n{}", class_name, java_source);
}
```

## Generated Output

The generated Java uses ASM to emit bytecode:

```java
import org.objectweb.asm.*;
import static org.objectweb.asm.Opcodes.*;

public class ArrayBuiltins {
    public static void build_Add(ClassWriter cw) {
        MethodVisitor mv = cw.visitMethod(
            ACC_PUBLIC | ACC_STATIC,
            "Add",
            "(II)I",
            null, null
        );
        mv.visitCode();
        mv.visitVarInsn(ILOAD, 0);  // a
        mv.visitVarInsn(ILOAD, 1);  // b
        mv.visitInsn(IADD);
        mv.visitInsn(IRETURN);
        mv.visitMaxs(-1, -1);
        mv.visitEnd();
    }
}
```

## Architecture

### Backend Trait

The codegen system uses a trait-based architecture allowing multiple backends:

```rust
pub trait Backend {
    type Type: Clone;
    type Value: Clone;
    type Label: Clone;
    type Output;

    fn emit_macro(&mut self, decl: &MacroDecl) -> CodeGenResult<()>;
    fn emit_builtin(&mut self, decl: &BuiltinDecl) -> CodeGenResult<()>;
    fn emit_expr(&mut self, expr: &Spanned<Expr>) -> CodeGenResult<Self::Value>;
    // ... more methods
}
```

### Current Backends

- `JavaAsmBackend`: Generates Java source with ASM library calls

### Planned Backends

- Direct bytecode emission
- WASM
- LLVM IR

## Supported Torque Constructs

- ✅ Namespaces
- ✅ Type declarations (class, struct, bitfield)
- ✅ Macros
- ✅ Builtins (javascript, runtime)
- ✅ Intrinsics
- ✅ Arithmetic expressions
- ✅ Bitwise operations
- ✅ Comparisons
- ✅ Control flow (if/else, while, for)
- ✅ Labels and goto
- ✅ Typeswitch
- ✅ Field access
- ✅ Method calls
- ✅ Try/catch (label handlers)

## License

MIT
