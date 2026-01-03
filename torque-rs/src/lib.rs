//! torque-rs: A Rust parser and code generator framework for V8 Torque
//!
//! This crate provides:
//! - A lexer and parser for V8 Torque `.tq` files
//! - AST types representing Torque programs
//! - Trait-based code generation for targeting different backends
//!
//! # Example
//!
//! ```ignore
//! use torque_rs::{parse_source, codegen::CodeGen};
//!
//! let source = r#"
//!     namespace math {
//!         javascript builtin MathAbs(
//!             context: Context, receiver: Object, x: Object): Number {
//!             const num: Number = ToNumber(context, x);
//!             return num < 0 ? -num : num;
//!         }
//!     }
//! "#;
//!
//! let ast = parse_source(source)?;
//! let mut gen = MyBackend::new();
//! let output = gen.generate(&ast)?;
//! ```

pub mod ast;
pub mod codegen;
pub mod lexer;
pub mod parser;

pub use ast::*;
pub use codegen::{CodeGen, CodeGenError, CodeGenResult};

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

/// Example codegen backend that prints the AST structure
pub mod examples {
    use super::*;
    use codegen::*;

    /// A simple backend that generates a debug representation
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
    }

    impl Default for DebugPrinter {
        fn default() -> Self {
            Self::new()
        }
    }

    impl CodeGen for DebugPrinter {
        type Output = String;
        type TypeRepr = String;
        type ExprResult = ();

        fn generate(&mut self, file: &SourceFile) -> CodeGenResult<Self::Output> {
            self.emit("SourceFile {");
            self.enter();
            for decl in &file.declarations {
                self.visit_declaration(&decl.node)?;
            }
            self.leave();
            self.emit("}");
            Ok(std::mem::take(&mut self.output))
        }

        fn visit_declaration(&mut self, decl: &Declaration) -> CodeGenResult<()> {
            match decl {
                Declaration::Namespace(ns) => {
                    self.emit(&format!("Namespace '{}' {{", ns.name.name));
                    self.enter();
                    for d in &ns.declarations {
                        self.visit_declaration(&d.node)?;
                    }
                    self.leave();
                    self.emit("}");
                }
                Declaration::Builtin(b) => {
                    let js = if b.is_javascript { "javascript " } else { "" };
                    self.emit(&format!("{}builtin {}(", js, b.name.name));
                    self.enter();
                    for p in &b.params {
                        self.emit(&format!("{}: {:?}", p.name.name, p.type_expr));
                    }
                    self.leave();
                    if let Some(ret) = &b.return_type {
                        self.emit(&format!("): {:?}", ret));
                    }
                    if let Some(body) = &b.body {
                        self.emit("Body {");
                        self.enter();
                        for stmt in &body.statements {
                            self.visit_statement(&stmt.node)?;
                        }
                        self.leave();
                        self.emit("}");
                    }
                }
                Declaration::Macro(m) => {
                    self.emit(&format!("macro {}(...)", m.name.name));
                }
                Declaration::Type(t) => {
                    self.emit(&format!("type {} ...", t.name.name));
                }
                Declaration::Const(c) => {
                    self.emit(&format!("const {}: {:?} = ...", c.name.name, c.type_expr));
                }
                Declaration::Extern(e) => {
                    self.emit(&format!("extern {:?}", e));
                }
                Declaration::Class(c) => {
                    self.emit(&format!("class {} ...", c.name.name));
                }
                Declaration::Struct(s) => {
                    self.emit(&format!("struct {} ...", s.name.name));
                }
            }
            Ok(())
        }

        fn visit_statement(&mut self, stmt: &Statement) -> CodeGenResult<()> {
            match stmt {
                Statement::Return(Some(e)) => {
                    self.emit("return");
                    self.enter();
                    self.visit_expr(&e.node)?;
                    self.leave();
                }
                Statement::Return(None) => {
                    self.emit("return;");
                }
                Statement::VarDecl { is_const, name, type_expr, init } => {
                    let kw = if *is_const { "const" } else { "let" };
                    self.emit(&format!("{} {}: {:?} = ...", kw, name.name, type_expr));
                }
                Statement::If { condition, then_branch, else_branch } => {
                    self.emit("if (...) {");
                    self.enter();
                    for s in &then_branch.statements {
                        self.visit_statement(&s.node)?;
                    }
                    self.leave();
                    if let Some(eb) = else_branch {
                        self.emit("} else {");
                        self.enter();
                        for s in &eb.statements {
                            self.visit_statement(&s.node)?;
                        }
                        self.leave();
                    }
                    self.emit("}");
                }
                Statement::Typeswitch { value, cases } => {
                    self.emit("typeswitch (...) {");
                    self.enter();
                    for case in cases {
                        self.emit(&format!("case ({}: {:?}):", case.binding.name, case.type_expr));
                    }
                    self.leave();
                    self.emit("}");
                }
                Statement::Expr(e) => {
                    self.visit_expr(&e.node)?;
                    self.emit(";");
                }
                _ => {
                    self.emit(&format!("{:?}", stmt));
                }
            }
            Ok(())
        }

        fn visit_expr(&mut self, expr: &Expr) -> CodeGenResult<()> {
            match expr {
                Expr::Ident(id) => self.emit(&format!("Ident({})", id.name)),
                Expr::IntLiteral(n) => self.emit(&format!("Int({})", n)),
                Expr::FloatLiteral(n) => self.emit(&format!("Float({})", n)),
                Expr::StringLiteral(s) => self.emit(&format!("String({:?})", s)),
                Expr::BoolLiteral(b) => self.emit(&format!("Bool({})", b)),
                Expr::Binary { op, left, right } => {
                    self.emit(&format!("Binary({:?})", op));
                    self.enter();
                    self.visit_expr(&left.node)?;
                    self.visit_expr(&right.node)?;
                    self.leave();
                }
                Expr::Call { callee, args, .. } => {
                    self.emit("Call");
                    self.enter();
                    self.visit_expr(&callee.node)?;
                    for arg in args {
                        self.visit_expr(&arg.node)?;
                    }
                    self.leave();
                }
                Expr::FieldAccess { object, field } => {
                    self.emit(&format!("FieldAccess(.{})", field.name));
                    self.enter();
                    self.visit_expr(&object.node)?;
                    self.leave();
                }
                Expr::Ternary { condition, then_expr, else_expr } => {
                    self.emit("Ternary");
                    self.enter();
                    self.visit_expr(&condition.node)?;
                    self.visit_expr(&then_expr.node)?;
                    self.visit_expr(&else_expr.node)?;
                    self.leave();
                }
                _ => self.emit(&format!("{:?}", expr)),
            }
            Ok(())
        }

        fn resolve_type(&mut self, ty: &TypeExpr) -> CodeGenResult<Self::TypeRepr> {
            Ok(format!("{:?}", ty))
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
}
