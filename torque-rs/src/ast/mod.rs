//! Abstract Syntax Tree for V8 Torque
//!
//! This module defines all AST node types that represent a parsed Torque program.

use smol_str::SmolStr;
use std::ops::Range;

use crate::lexer::F64Wrapper;

/// A span in the source code
pub type Span = Range<usize>;

/// An identifier with its source location
#[derive(Debug, Clone, PartialEq)]
pub struct Ident {
    pub name: SmolStr,
    pub span: Span,
}

impl Ident {
    pub fn new(name: impl Into<SmolStr>, span: Span) -> Self {
        Self { name: name.into(), span }
    }
}

/// A spanned AST node
#[derive(Debug, Clone, PartialEq)]
pub struct Spanned<T> {
    pub node: T,
    pub span: Span,
}

impl<T> Spanned<T> {
    pub fn new(node: T, span: Span) -> Self {
        Self { node, span }
    }
}

// ============================================================================
// Top-Level Declarations
// ============================================================================

/// A complete Torque source file
#[derive(Debug, Clone, PartialEq)]
pub struct SourceFile {
    pub declarations: Vec<Spanned<Declaration>>,
}

/// Top-level declarations
#[derive(Debug, Clone, PartialEq)]
pub enum Declaration {
    /// `namespace foo { ... }`
    Namespace(NamespaceDecl),

    /// `type Foo extends Bar generates 'TNode<T>' constexpr 'T';`
    Type(TypeDecl),

    /// `macro Foo(...): T { ... }`
    Macro(MacroDecl),

    /// `builtin Foo(...): T { ... }`
    Builtin(BuiltinDecl),

    /// `extern macro/builtin/runtime ...`
    Extern(ExternDecl),

    /// `const kFoo: Type = value;`
    Const(ConstDecl),

    /// `class Foo extends Bar { ... }`
    Class(ClassDecl),

    /// `struct Foo { ... }`
    Struct(StructDecl),
}

#[derive(Debug, Clone, PartialEq)]
pub struct NamespaceDecl {
    pub name: Ident,
    pub declarations: Vec<Spanned<Declaration>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeDecl {
    pub name: Ident,
    pub type_params: Vec<Ident>,
    pub extends: Option<TypeExpr>,
    pub generates: Option<String>,
    pub constexpr: Option<String>,
    pub is_transient: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MacroDecl {
    pub annotations: Vec<Annotation>,
    pub name: Ident,
    pub type_params: Vec<Ident>,
    pub params: Vec<Parameter>,
    pub return_type: Option<TypeExpr>,
    pub labels: Vec<LabelDecl>,
    pub body: Option<Block>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BuiltinDecl {
    pub annotations: Vec<Annotation>,
    pub is_javascript: bool,
    pub name: Ident,
    pub type_params: Vec<Ident>,
    pub params: Vec<Parameter>,
    pub return_type: Option<TypeExpr>,
    pub body: Option<Block>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExternDecl {
    Macro(MacroDecl),
    Builtin(BuiltinDecl),
    Runtime(RuntimeDecl),
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeDecl {
    pub name: Ident,
    pub params: Vec<Parameter>,
    pub return_type: Option<TypeExpr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConstDecl {
    pub name: Ident,
    pub type_expr: TypeExpr,
    pub value: Spanned<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClassDecl {
    pub annotations: Vec<Annotation>,
    pub is_extern: bool,
    pub name: Ident,
    pub type_params: Vec<Ident>,
    pub extends: Option<TypeExpr>,
    pub fields: Vec<ClassField>,
    pub methods: Vec<Spanned<Declaration>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClassField {
    pub annotations: Vec<Annotation>,
    pub name: Ident,
    pub type_expr: TypeExpr,
    pub is_weak: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructDecl {
    pub name: Ident,
    pub type_params: Vec<Ident>,
    pub fields: Vec<StructField>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructField {
    pub name: Ident,
    pub type_expr: TypeExpr,
}

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
pub enum TypeExpr {
    /// Simple type: `Foo`
    Named(Ident),

    /// Generic type: `Foo<T, U>`
    Generic {
        name: Ident,
        args: Vec<TypeExpr>,
    },

    /// Union type: `Smi | HeapNumber`
    Union(Vec<TypeExpr>),

    /// Function type: `(A, B) => C`
    Function {
        params: Vec<TypeExpr>,
        return_type: Box<TypeExpr>,
    },

    /// Reference type: `&T`
    Reference(Box<TypeExpr>),
}

// ============================================================================
// Parameters and Labels
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
pub struct Parameter {
    pub name: Ident,
    pub type_expr: TypeExpr,
    pub is_implicit: bool,
    pub is_rest: bool,  // ...args
}

#[derive(Debug, Clone, PartialEq)]
pub struct LabelDecl {
    pub name: Ident,
    pub params: Vec<TypeExpr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Annotation {
    pub name: Ident,
    pub args: Vec<Spanned<Expr>>,
}

// ============================================================================
// Statements
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub statements: Vec<Spanned<Statement>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    /// `let x: T = expr;` or `const x: T = expr;`
    VarDecl {
        is_const: bool,
        name: Ident,
        type_expr: Option<TypeExpr>,
        init: Spanned<Expr>,
    },

    /// Expression as statement: `foo();`
    Expr(Spanned<Expr>),

    /// `return expr;`
    Return(Option<Spanned<Expr>>),

    /// `if (cond) { ... } else { ... }`
    If {
        condition: Spanned<Expr>,
        then_branch: Block,
        else_branch: Option<Block>,
    },

    /// `while (cond) { ... }`
    While {
        condition: Spanned<Expr>,
        body: Block,
    },

    /// `for (init; cond; update) { ... }`
    For {
        init: Option<Box<Spanned<Statement>>>,
        condition: Option<Spanned<Expr>>,
        update: Option<Spanned<Expr>>,
        body: Block,
    },

    /// `typeswitch (expr) { case (x: T): { ... } }`
    Typeswitch {
        value: Spanned<Expr>,
        cases: Vec<TypeswitchCase>,
    },

    /// `try { ... } label Foo { ... }`
    TryLabel {
        try_block: Block,
        labels: Vec<LabelBlock>,
    },

    /// `goto LabelName(args);`
    Goto {
        label: Ident,
        args: Vec<Spanned<Expr>>,
    },

    /// `break;`
    Break,

    /// `continue;`
    Continue,

    /// `tail CallExpr;`
    Tail(Spanned<Expr>),

    /// `dcheck(expr);`
    Dcheck(Spanned<Expr>),

    /// `check(expr);`
    Check(Spanned<Expr>),

    /// `unreachable;`
    Unreachable,

    /// Nested block `{ ... }`
    Block(Block),
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeswitchCase {
    pub binding: Ident,
    pub type_expr: TypeExpr,
    pub body: Block,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LabelBlock {
    pub name: Ident,
    pub params: Vec<Parameter>,
    pub body: Block,
}

// ============================================================================
// Expressions
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// Identifier: `foo`
    Ident(Ident),

    /// Integer literal: `42`
    IntLiteral(i64),

    /// Float literal: `3.14`
    FloatLiteral(F64Wrapper),

    /// String literal: `"hello"`
    StringLiteral(String),

    /// Boolean: `true` / `false`
    BoolLiteral(bool),

    /// Binary operation: `a + b`
    Binary {
        op: BinaryOp,
        left: Box<Spanned<Expr>>,
        right: Box<Spanned<Expr>>,
    },

    /// Unary operation: `!x`, `-x`
    Unary {
        op: UnaryOp,
        operand: Box<Spanned<Expr>>,
    },

    /// Ternary: `cond ? then : else`
    Ternary {
        condition: Box<Spanned<Expr>>,
        then_expr: Box<Spanned<Expr>>,
        else_expr: Box<Spanned<Expr>>,
    },

    /// Function/macro call: `Foo(a, b)`
    Call {
        callee: Box<Spanned<Expr>>,
        type_args: Vec<TypeExpr>,
        args: Vec<Spanned<Expr>>,
        otherwise: Vec<Ident>,  // `otherwise Label1, Label2`
    },

    /// Intrinsic call: `%RawDownCast<T>(x)`
    Intrinsic {
        name: Ident,
        type_args: Vec<TypeExpr>,
        args: Vec<Spanned<Expr>>,
    },

    /// Field access: `obj.field`
    FieldAccess {
        object: Box<Spanned<Expr>>,
        field: Ident,
    },

    /// Index access: `arr[i]`
    Index {
        object: Box<Spanned<Expr>>,
        index: Box<Spanned<Expr>>,
    },

    /// Assignment: `x = y`
    Assign {
        target: Box<Spanned<Expr>>,
        value: Box<Spanned<Expr>>,
    },

    /// Compound assignment: `x += y`
    CompoundAssign {
        op: BinaryOp,
        target: Box<Spanned<Expr>>,
        value: Box<Spanned<Expr>>,
    },

    /// Increment/decrement: `x++`, `--x`
    Increment {
        operand: Box<Spanned<Expr>>,
        is_prefix: bool,
        is_decrement: bool,
    },

    /// New expression: `new Foo { field: value }`
    New {
        type_expr: TypeExpr,
        fields: Vec<(Ident, Spanned<Expr>)>,
    },

    /// Type assertion: `expr as Type`
    As {
        expr: Box<Spanned<Expr>>,
        type_expr: TypeExpr,
    },

    /// Is expression: `Is<Type>(expr)`
    Is {
        type_expr: TypeExpr,
        expr: Box<Spanned<Expr>>,
    },

    /// Cast: `Convert<Type>(expr)` or `UnsafeCast<Type>(expr)`
    Convert {
        kind: ConvertKind,
        type_expr: TypeExpr,
        expr: Box<Spanned<Expr>>,
    },

    /// Parenthesized: `(expr)`
    Paren(Box<Spanned<Expr>>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    // Arithmetic
    Add, Sub, Mul, Div, Mod,
    // Comparison
    Eq, Ne, Lt, Le, Gt, Ge,
    // Logical
    And, Or,
    // Bitwise
    BitAnd, BitOr, BitXor,
    Shl, Shr, Ushr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,     // -x
    Not,     // !x
    BitNot,  // ~x
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConvertKind {
    Convert,     // Safe conversion
    UnsafeCast,  // Unchecked cast
}
