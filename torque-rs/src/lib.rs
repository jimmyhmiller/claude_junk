//! torque-rs: A Rust parser and code generator framework for V8 Torque
//!
//! This crate provides:
//! - A lexer and parser for V8 Torque `.tq` files
//! - AST types representing Torque programs
//! - Trait-based code generation for targeting different backends (JVM, WASM, etc.)
//!
//! # Example
//!
//! ```ignore
//! use torque_rs::{parse_source, CodeGenerator};
//! use torque_rs::codegen::java_asm::JavaAsmBackend;
//!
//! let source = r#"
//!     namespace array {
//!         javascript builtin ArrayPush(
//!             context: Context, receiver: JSArray, ...args): Number {
//!             return receiver.length;
//!         }
//!     }
//! "#;
//!
//! let ast = parse_source(source)?;
//! let backend = JavaAsmBackend::new("js.builtins");
//! let mut gen = CodeGenerator::new(backend);
//! let output = gen.generate(&ast)?;
//! // output.classes contains Java source files that use ASM to emit bytecode
//! ```

pub mod ast;
pub mod codegen;
pub mod lexer;
pub mod parser;

pub use ast::*;
pub use codegen::{Backend, CodeGenError, CodeGenResult, CodeGenerator, JvmType};

use lexer::{lex, Token};
use parser::parse;

/// Parse a Torque source string into an AST
pub fn parse_source(source: &str) -> Result<SourceFile, ParseError> {
    // Step 1: Lexing
    let tokens = lex(source).map_err(|e| ParseError::LexError {
        span: e.span,
        message: e.message,
    })?;

    // Step 2: Convert to parser input format
    let parser_input: Vec<(Token, std::ops::Range<usize>)> = tokens
        .into_iter()
        .map(|t| (t.token, t.span))
        .collect();

    // Step 3: Parsing
    parse(parser_input).map_err(|errs| ParseError::ParseErrors(errs))
}

/// Errors that can occur during parsing
#[derive(Debug)]
pub enum ParseError {
    LexError {
        span: std::ops::Range<usize>,
        message: String,
    },
    ParseErrors(Vec<chumsky::error::Simple<Token>>),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::LexError { span, message } => {
                write!(f, "Lex error at {:?}: {}", span, message)
            }
            ParseError::ParseErrors(errs) => {
                for err in errs {
                    writeln!(f, "Parse error: {:?}", err)?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for ParseError {}

// ============================================================================
// Example: Debug Printer Backend
// ============================================================================

/// Example: Debug printer that walks the AST
pub mod examples {
    use super::*;

    /// A simple AST printer for debugging
    pub struct DebugPrinter {
        output: String,
        indent: usize,
    }

    impl DebugPrinter {
        pub fn new() -> Self {
            Self {
                output: String::new(),
                indent: 0,
            }
        }

        fn emit(&mut self, s: &str) {
            for _ in 0..self.indent {
                self.output.push_str("  ");
            }
            self.output.push_str(s);
            self.output.push('\n');
        }

        fn enter(&mut self) {
            self.indent += 1;
        }

        fn leave(&mut self) {
            self.indent = self.indent.saturating_sub(1);
        }

        pub fn print(&mut self, file: &SourceFile) -> String {
            self.emit("SourceFile {");
            self.enter();
            for decl in &file.declarations {
                self.print_declaration(&decl.node);
            }
            self.leave();
            self.emit("}");
            std::mem::take(&mut self.output)
        }

        fn print_declaration(&mut self, decl: &Declaration) {
            match decl {
                Declaration::Namespace(ns) => {
                    self.emit(&format!("Namespace '{}' {{", ns.name.name));
                    self.enter();
                    for d in &ns.declarations {
                        self.print_declaration(&d.node);
                    }
                    self.leave();
                    self.emit("}");
                }
                Declaration::Builtin(b) => {
                    let js = if b.is_javascript { "javascript " } else { "" };
                    self.emit(&format!("{}builtin {}(...)", js, b.name.name));
                }
                Declaration::Macro(m) => {
                    self.emit(&format!("macro {}(...)", m.name.name));
                }
                Declaration::Type(t) => {
                    self.emit(&format!("type {} ...", t.name.name));
                }
                Declaration::Const(c) => {
                    self.emit(&format!("const {} ...", c.name.name));
                }
                Declaration::Extern(_e) => {
                    self.emit("extern ...");
                }
                Declaration::Class(c) => {
                    self.emit(&format!("class {} ...", c.name.name));
                }
                Declaration::Struct(s) => {
                    self.emit(&format!("struct {} ...", s.name.name));
                }
            }
        }
    }

    impl Default for DebugPrinter {
        fn default() -> Self {
            Self::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lexer_basic() {
        let source = "namespace foo { }";
        let tokens = lexer::lex(source).unwrap();
        assert_eq!(tokens.len(), 4); // namespace, foo, {, }
    }

    #[test]
    fn test_lexer_builtin() {
        let source = "javascript builtin Foo(x: Number): Object";
        let tokens = lexer::lex(source).unwrap();
        assert!(tokens.len() > 0);
        assert!(matches!(tokens[0].token, lexer::Token::Javascript));
        assert!(matches!(tokens[1].token, lexer::Token::Builtin));
    }

    #[test]
    fn test_parse_namespace() {
        let source = "namespace test { }";
        let result = parse_source(source);
        assert!(result.is_ok(), "Failed to parse: {:?}", result.err());
        let ast = result.unwrap();
        assert_eq!(ast.declarations.len(), 1);
        match &ast.declarations[0].node {
            Declaration::Namespace(ns) => {
                assert_eq!(ns.name.name.as_str(), "test");
            }
            _ => panic!("Expected namespace declaration"),
        }
    }

    #[test]
    fn test_parse_builtin() {
        let source = r#"
            javascript builtin MathAbs(context: Context, x: Number): Number {
                return x;
            }
        "#;
        let result = parse_source(source);
        assert!(result.is_ok(), "Failed to parse: {:?}", result.err());
        let ast = result.unwrap();
        assert_eq!(ast.declarations.len(), 1);
        match &ast.declarations[0].node {
            Declaration::Builtin(b) => {
                assert!(b.is_javascript);
                assert_eq!(b.name.name.as_str(), "MathAbs");
                assert_eq!(b.params.len(), 2);
            }
            _ => panic!("Expected builtin declaration"),
        }
    }

    #[test]
    fn test_parse_binary_expr() {
        let source = r#"
            macro Test(): Number {
                return 1 + 2 * 3;
            }
        "#;
        let result = parse_source(source);
        assert!(result.is_ok(), "Failed to parse: {:?}", result.err());
    }

    #[test]
    fn test_parse_complex_expr() {
        let source = r#"
            macro Complex(): Number {
                return a + b * c - d / e && f || g;
            }
        "#;
        let result = parse_source(source);
        assert!(result.is_ok(), "Failed to parse: {:?}", result.err());
    }

    #[test]
    fn test_parse_comprehensive_file() {
        let source = include_str!("../tests/comprehensive.tq");
        let result = parse_source(source);
        assert!(result.is_ok(), "Failed to parse comprehensive.tq: {:?}", result.err());
        let ast = result.unwrap();
        // Should have multiple top-level declarations
        assert!(ast.declarations.len() > 10, "Expected many declarations, got {}", ast.declarations.len());
    }

    #[test]
    fn test_parse_stress_test_file() {
        let source = include_str!("../tests/stress_test.tq");
        println!("Parsing {} bytes, {} lines", source.len(), source.lines().count());
        let start = std::time::Instant::now();
        let result = parse_source(source);
        let elapsed = start.elapsed();
        println!("Parsing took {:?}", elapsed);
        assert!(result.is_ok(), "Failed to parse stress_test.tq: {:?}", result.err());
        let ast = result.unwrap();
        println!("Parsed {} declarations", ast.declarations.len());
        // Should have 200 types + 200 consts + 400 macros + 200 builtins + 50 namespaces = 1050
        assert!(ast.declarations.len() >= 1000, "Expected ~1050 declarations, got {}", ast.declarations.len());
    }
}
