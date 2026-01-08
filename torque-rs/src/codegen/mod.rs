//! Code Generation Traits for Torque
//!
//! This module defines a trait-based system for generating code from Torque AST.
//! Implement the `Backend` trait to target different platforms (JVM, WASM, LLVM, etc.)

pub mod java_asm;

use crate::ast::*;
use crate::lexer::F64Wrapper;
use std::collections::HashMap;

/// Result type for code generation operations
pub type CodeGenResult<T> = Result<T, CodeGenError>;

/// Errors that can occur during code generation
#[derive(Debug, Clone, thiserror::Error)]
pub enum CodeGenError {
    #[error("Unsupported feature: {0}")]
    Unsupported(String),

    #[error("Type error: {0}")]
    TypeError(String),

    #[error("Unknown identifier: {0}")]
    UnknownIdent(String),

    #[error("Unknown type: {0}")]
    UnknownType(String),

    #[error("Unknown label: {0}")]
    UnknownLabel(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

// ============================================================================
// Core Backend Trait
// ============================================================================

/// The main trait that all code generation backends must implement.
///
/// A Backend defines how to translate Torque constructs to a target platform.
/// The trait uses associated types to allow backends to define their own
/// representations for types, values, and labels.
///
/// # Example
///
/// ```ignore
/// struct WasmBackend { ... }
///
/// impl Backend for WasmBackend {
///     type Type = WasmType;
///     type Value = WasmValue;
///     type Label = WasmLabel;
///     type Output = Vec<u8>;  // WASM bytecode
///
///     fn emit_class(&mut self, decl: &ClassDecl) -> CodeGenResult<()> {
///         // Generate WASM struct type
///     }
///     // ...
/// }
/// ```
pub trait Backend {
    /// How types are represented in this backend
    type Type: Clone;

    /// How values/expressions are represented (e.g., register, stack slot)
    type Value: Clone;

    /// How labels are represented for control flow
    type Label: Clone;

    /// The final output produced by the backend
    type Output;

    // ========== Type System ==========

    /// Map a Torque type expression to the backend's type representation
    fn map_type(&mut self, ty: &TypeExpr) -> CodeGenResult<Self::Type>;

    /// Get the type of a primitive
    fn primitive_type(&self, name: &str) -> CodeGenResult<Self::Type>;

    /// Create a union type
    fn union_type(&mut self, types: &[Self::Type]) -> CodeGenResult<Self::Type>;

    /// Create a function type
    fn function_type(
        &mut self,
        params: &[Self::Type],
        return_type: &Self::Type,
    ) -> CodeGenResult<Self::Type>;

    // ========== Declarations ==========

    /// Emit a namespace (may create a class/module in the target)
    fn emit_namespace(&mut self, decl: &NamespaceDecl) -> CodeGenResult<()>;

    /// Emit a type alias/declaration
    fn emit_type_decl(&mut self, decl: &TypeDecl) -> CodeGenResult<()>;

    /// Emit a macro (typically an inlined function)
    fn emit_macro(&mut self, decl: &MacroDecl) -> CodeGenResult<()>;

    /// Emit a builtin (a callable function)
    fn emit_builtin(&mut self, decl: &BuiltinDecl) -> CodeGenResult<()>;

    /// Emit an extern declaration
    fn emit_extern(&mut self, decl: &ExternDecl) -> CodeGenResult<()>;

    /// Emit a constant
    fn emit_const(&mut self, decl: &ConstDecl) -> CodeGenResult<()>;

    /// Emit a class definition
    fn emit_class(&mut self, decl: &ClassDecl) -> CodeGenResult<()>;

    /// Emit a struct definition
    fn emit_struct(&mut self, decl: &StructDecl) -> CodeGenResult<()>;

    // ========== Statements ==========

    /// Emit a variable declaration
    fn emit_var_decl(
        &mut self,
        is_const: bool,
        name: &Ident,
        ty: Option<&TypeExpr>,
        init: &Spanned<Expr>,
    ) -> CodeGenResult<()>;

    /// Emit a return statement
    fn emit_return(&mut self, value: Option<&Spanned<Expr>>) -> CodeGenResult<()>;

    /// Emit an if statement
    fn emit_if(
        &mut self,
        condition: &Spanned<Expr>,
        then_branch: &Block,
        else_branch: Option<&Block>,
    ) -> CodeGenResult<()>;

    /// Emit a while loop
    fn emit_while(&mut self, condition: &Spanned<Expr>, body: &Block) -> CodeGenResult<()>;

    /// Emit a for loop
    fn emit_for(
        &mut self,
        init: Option<&Spanned<Statement>>,
        condition: Option<&Spanned<Expr>>,
        update: Option<&Spanned<Expr>>,
        body: &Block,
    ) -> CodeGenResult<()>;

    /// Emit a typeswitch statement
    fn emit_typeswitch(
        &mut self,
        value: &Spanned<Expr>,
        cases: &[TypeswitchCase],
    ) -> CodeGenResult<()>;

    /// Emit a try/label block
    fn emit_try(
        &mut self,
        body: &Block,
        handlers: &[LabelBlock],
    ) -> CodeGenResult<()>;

    /// Emit a goto statement
    fn emit_goto(&mut self, label: &Ident, args: &[Spanned<Expr>]) -> CodeGenResult<()>;

    /// Emit break
    fn emit_break(&mut self) -> CodeGenResult<()>;

    /// Emit continue
    fn emit_continue(&mut self) -> CodeGenResult<()>;

    /// Emit unreachable
    fn emit_unreachable(&mut self) -> CodeGenResult<()>;

    /// Emit an assertion
    fn emit_assert(&mut self, kind: AssertKind, condition: &Spanned<Expr>) -> CodeGenResult<()>;

    // ========== Expressions ==========

    /// Emit an expression, returning the value representation
    fn emit_expr(&mut self, expr: &Spanned<Expr>) -> CodeGenResult<Self::Value>;

    /// Emit an identifier reference
    fn emit_ident(&mut self, ident: &Ident) -> CodeGenResult<Self::Value>;

    /// Emit a literal
    fn emit_literal(&mut self, lit: &Expr) -> CodeGenResult<Self::Value>;

    /// Emit a binary operation
    fn emit_binary(
        &mut self,
        op: BinaryOp,
        left: &Spanned<Expr>,
        right: &Spanned<Expr>,
    ) -> CodeGenResult<Self::Value>;

    /// Emit a unary operation
    fn emit_unary(&mut self, op: UnaryOp, operand: &Spanned<Expr>) -> CodeGenResult<Self::Value>;

    /// Emit a function/macro call
    fn emit_call(
        &mut self,
        callee: &Spanned<Expr>,
        type_args: &[TypeExpr],
        args: &[Spanned<Expr>],
        otherwise: &[Ident],
    ) -> CodeGenResult<Self::Value>;

    /// Emit a field access
    fn emit_field_access(
        &mut self,
        object: &Spanned<Expr>,
        field: &Ident,
    ) -> CodeGenResult<Self::Value>;

    /// Emit an index operation
    fn emit_index(
        &mut self,
        object: &Spanned<Expr>,
        index: &Spanned<Expr>,
    ) -> CodeGenResult<Self::Value>;

    /// Emit an assignment
    fn emit_assign(
        &mut self,
        target: &Spanned<Expr>,
        value: &Spanned<Expr>,
    ) -> CodeGenResult<Self::Value>;

    /// Emit a new expression
    fn emit_new(
        &mut self,
        type_expr: &TypeExpr,
        fields: &[(Ident, Spanned<Expr>)],
    ) -> CodeGenResult<Self::Value>;

    // ========== Labels ==========

    /// Declare a label
    fn declare_label(&mut self, name: &Ident, params: &[Parameter]) -> CodeGenResult<Self::Label>;

    /// Emit a jump to a label
    fn emit_jump(&mut self, label: &Self::Label, args: &[Spanned<Expr>]) -> CodeGenResult<()>;

    /// Begin a label's body
    fn begin_label(&mut self, label: &Self::Label) -> CodeGenResult<()>;

    /// End a label's body
    fn end_label(&mut self, label: &Self::Label) -> CodeGenResult<()>;

    // ========== Finalization ==========

    /// Generate the final output from all emitted code
    fn finalize(&mut self) -> CodeGenResult<Self::Output>;
}

// ============================================================================
// Code Generator - Orchestrates the Backend
// ============================================================================

/// The main code generator that uses a Backend to produce output.
pub struct CodeGenerator<B: Backend> {
    pub backend: B,
    /// Namespace stack for nested namespaces
    namespace_stack: Vec<String>,
    /// Type environment
    type_env: HashMap<String, B::Type>,
    /// Variable environment
    var_env: HashMap<String, B::Value>,
    /// Label environment
    label_env: HashMap<String, B::Label>,
}

impl<B: Backend> CodeGenerator<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            namespace_stack: Vec::new(),
            type_env: HashMap::new(),
            var_env: HashMap::new(),
            label_env: HashMap::new(),
        }
    }

    /// Generate code for an entire source file
    pub fn generate(&mut self, file: &SourceFile) -> CodeGenResult<B::Output> {
        for decl in &file.declarations {
            self.visit_declaration(&decl.node)?;
        }
        self.backend.finalize()
    }

    /// Visit a declaration
    pub fn visit_declaration(&mut self, decl: &Declaration) -> CodeGenResult<()> {
        match decl {
            Declaration::Namespace(d) => {
                self.namespace_stack.push(d.name.name.to_string());
                self.backend.emit_namespace(d)?;
                for inner in &d.declarations {
                    self.visit_declaration(&inner.node)?;
                }
                self.namespace_stack.pop();
            }
            Declaration::Type(d) => self.backend.emit_type_decl(d)?,
            Declaration::Macro(d) => self.backend.emit_macro(d)?,
            Declaration::Builtin(d) => self.backend.emit_builtin(d)?,
            Declaration::Extern(d) => self.backend.emit_extern(d)?,
            Declaration::Const(d) => self.backend.emit_const(d)?,
            Declaration::Class(d) => self.backend.emit_class(d)?,
            Declaration::Struct(d) => self.backend.emit_struct(d)?,
        }
        Ok(())
    }

    /// Visit a statement
    pub fn visit_statement(&mut self, stmt: &Statement) -> CodeGenResult<()> {
        match stmt {
            Statement::VarDecl { is_const, name, type_expr, init } => {
                self.backend.emit_var_decl(*is_const, name, type_expr.as_ref(), init)?;
            }
            Statement::Expr(e) => {
                self.backend.emit_expr(e)?;
            }
            Statement::Return(v) => self.backend.emit_return(v.as_ref())?,
            Statement::If { condition, then_branch, else_branch } => {
                self.backend.emit_if(condition, then_branch, else_branch.as_ref())?;
            }
            Statement::While { condition, body } => {
                self.backend.emit_while(condition, body)?;
            }
            Statement::For { init, condition, update, body } => {
                self.backend.emit_for(
                    init.as_ref().map(|b| b.as_ref()),
                    condition.as_ref(),
                    update.as_ref(),
                    body,
                )?;
            }
            Statement::Typeswitch { value, cases } => {
                self.backend.emit_typeswitch(value, cases)?;
            }
            Statement::Try { body, handlers } => {
                self.backend.emit_try(body, handlers)?;
            }
            Statement::Goto { label, args } => {
                self.backend.emit_goto(label, args)?;
            }
            Statement::Break => self.backend.emit_break()?,
            Statement::Continue => self.backend.emit_continue()?,
            Statement::Unreachable => self.backend.emit_unreachable()?,
            Statement::Assert { kind, condition } => {
                self.backend.emit_assert(*kind, condition)?;
            }
            Statement::Block(b) => {
                for stmt in &b.statements {
                    self.visit_statement(&stmt.node)?;
                }
            }
            Statement::Tail(e) => {
                self.backend.emit_expr(e)?;
                self.backend.emit_return(Some(e))?;
            }
        }
        Ok(())
    }

    /// Get current namespace path
    pub fn current_namespace(&self) -> String {
        self.namespace_stack.join("::")
    }
}

// ============================================================================
// JVM Type Descriptors (shared by Java backends)
// ============================================================================

/// JVM type descriptor
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum JvmType {
    Void,
    Boolean,
    Byte,
    Char,
    Short,
    Int,
    Long,
    Float,
    Double,
    Object(String),  // Internal name: "java/lang/Object"
    Array(Box<JvmType>),
}

impl JvmType {
    /// Get the JVM descriptor string
    pub fn descriptor(&self) -> String {
        match self {
            JvmType::Void => "V".to_string(),
            JvmType::Boolean => "Z".to_string(),
            JvmType::Byte => "B".to_string(),
            JvmType::Char => "C".to_string(),
            JvmType::Short => "S".to_string(),
            JvmType::Int => "I".to_string(),
            JvmType::Long => "J".to_string(),
            JvmType::Float => "F".to_string(),
            JvmType::Double => "D".to_string(),
            JvmType::Object(name) => format!("L{};", name),
            JvmType::Array(elem) => format!("[{}", elem.descriptor()),
        }
    }

    /// Get the internal name (for Object types)
    pub fn internal_name(&self) -> String {
        match self {
            JvmType::Object(name) => name.clone(),
            _ => panic!("internal_name only valid for Object types"),
        }
    }

    /// Stack size (1 for most types, 2 for long/double)
    pub fn stack_size(&self) -> usize {
        match self {
            JvmType::Long | JvmType::Double => 2,
            JvmType::Void => 0,
            _ => 1,
        }
    }

    /// Common Torque type mappings
    pub fn from_torque(name: &str) -> Option<Self> {
        match name {
            // Primitives
            "void" | "Void" => Some(JvmType::Void),
            "bool" | "Boolean" => Some(JvmType::Boolean),
            "int32" | "Int32" => Some(JvmType::Int),
            "int64" | "Int64" => Some(JvmType::Long),
            "uint32" | "Uint32" => Some(JvmType::Int),
            "uint64" | "Uint64" => Some(JvmType::Long),
            "float32" | "Float32" => Some(JvmType::Float),
            "float64" | "Float64" | "Number" => Some(JvmType::Double),
            "intptr" | "Intptr" | "uintptr" | "Uintptr" => Some(JvmType::Long),

            // Tagged values -> Object
            "Smi" => Some(JvmType::Object("js/runtime/Smi".to_string())),
            "HeapNumber" => Some(JvmType::Object("js/runtime/HeapNumber".to_string())),
            "HeapObject" => Some(JvmType::Object("js/runtime/HeapObject".to_string())),
            "Object" | "JSAny" => Some(JvmType::Object("java/lang/Object".to_string())),

            // JavaScript types
            "String" => Some(JvmType::Object("js/runtime/JSString".to_string())),
            "JSArray" => Some(JvmType::Object("js/runtime/JSArray".to_string())),
            "JSObject" => Some(JvmType::Object("js/runtime/JSObject".to_string())),
            "JSFunction" => Some(JvmType::Object("js/runtime/JSFunction".to_string())),
            "Map" => Some(JvmType::Object("js/runtime/Map".to_string())),
            "FixedArray" => Some(JvmType::Object("js/runtime/FixedArray".to_string())),
            "Context" => Some(JvmType::Object("js/runtime/Context".to_string())),

            // Never type
            "never" | "Never" => Some(JvmType::Void),

            _ => None,
        }
    }
}

/// Helper to build method descriptors
pub fn method_descriptor(params: &[JvmType], return_type: &JvmType) -> String {
    let params_desc: String = params.iter().map(|t| t.descriptor()).collect();
    format!("({}){}", params_desc, return_type.descriptor())
}

// ============================================================================
// Visitor Traits (for backwards compatibility)
// ============================================================================

/// Trait for visiting declarations
pub trait DeclVisitor {
    type Result;

    fn visit_namespace(&mut self, decl: &NamespaceDecl) -> CodeGenResult<Self::Result>;
    fn visit_type_decl(&mut self, decl: &TypeDecl) -> CodeGenResult<Self::Result>;
    fn visit_macro(&mut self, decl: &MacroDecl) -> CodeGenResult<Self::Result>;
    fn visit_builtin(&mut self, decl: &BuiltinDecl) -> CodeGenResult<Self::Result>;
    fn visit_extern(&mut self, decl: &ExternDecl) -> CodeGenResult<Self::Result>;
    fn visit_const(&mut self, decl: &ConstDecl) -> CodeGenResult<Self::Result>;
    fn visit_class(&mut self, decl: &ClassDecl) -> CodeGenResult<Self::Result>;
    fn visit_struct(&mut self, decl: &StructDecl) -> CodeGenResult<Self::Result>;
}

/// Trait for visiting statements
pub trait StmtVisitor {
    type Result;

    fn visit_var_decl(
        &mut self,
        is_const: bool,
        name: &Ident,
        type_expr: Option<&TypeExpr>,
        init: &Spanned<Expr>,
    ) -> CodeGenResult<Self::Result>;

    fn visit_expr_stmt(&mut self, expr: &Spanned<Expr>) -> CodeGenResult<Self::Result>;
    fn visit_return(&mut self, value: Option<&Spanned<Expr>>) -> CodeGenResult<Self::Result>;

    fn visit_if(
        &mut self,
        condition: &Spanned<Expr>,
        then_branch: &Block,
        else_branch: Option<&Block>,
    ) -> CodeGenResult<Self::Result>;

    fn visit_while(
        &mut self,
        condition: &Spanned<Expr>,
        body: &Block,
    ) -> CodeGenResult<Self::Result>;

    fn visit_for(
        &mut self,
        init: Option<&Spanned<Statement>>,
        condition: Option<&Spanned<Expr>>,
        update: Option<&Spanned<Expr>>,
        body: &Block,
    ) -> CodeGenResult<Self::Result>;

    fn visit_typeswitch(
        &mut self,
        value: &Spanned<Expr>,
        cases: &[TypeswitchCase],
    ) -> CodeGenResult<Self::Result>;

    fn visit_try_label(
        &mut self,
        try_block: &Block,
        labels: &[LabelBlock],
    ) -> CodeGenResult<Self::Result>;

    fn visit_goto(
        &mut self,
        label: &Ident,
        args: &[Spanned<Expr>],
    ) -> CodeGenResult<Self::Result>;

    fn visit_break(&mut self) -> CodeGenResult<Self::Result>;
    fn visit_continue(&mut self) -> CodeGenResult<Self::Result>;
    fn visit_unreachable(&mut self) -> CodeGenResult<Self::Result>;
    fn visit_block(&mut self, block: &Block) -> CodeGenResult<Self::Result>;
}

/// Trait for visiting expressions
pub trait ExprVisitor {
    type Result;

    fn visit_ident(&mut self, ident: &Ident) -> CodeGenResult<Self::Result>;
    fn visit_int_literal(&mut self, value: i64) -> CodeGenResult<Self::Result>;
    fn visit_float_literal(&mut self, value: F64Wrapper) -> CodeGenResult<Self::Result>;
    fn visit_string_literal(&mut self, value: &str) -> CodeGenResult<Self::Result>;
    fn visit_bool_literal(&mut self, value: bool) -> CodeGenResult<Self::Result>;

    fn visit_binary(
        &mut self,
        op: BinaryOp,
        left: &Spanned<Expr>,
        right: &Spanned<Expr>,
    ) -> CodeGenResult<Self::Result>;

    fn visit_unary(
        &mut self,
        op: UnaryOp,
        operand: &Spanned<Expr>,
    ) -> CodeGenResult<Self::Result>;

    fn visit_ternary(
        &mut self,
        condition: &Spanned<Expr>,
        then_expr: &Spanned<Expr>,
        else_expr: &Spanned<Expr>,
    ) -> CodeGenResult<Self::Result>;

    fn visit_call(
        &mut self,
        callee: &Spanned<Expr>,
        type_args: &[TypeExpr],
        args: &[Spanned<Expr>],
        otherwise: &[Ident],
    ) -> CodeGenResult<Self::Result>;

    fn visit_intrinsic(
        &mut self,
        name: &Ident,
        type_args: &[TypeExpr],
        args: &[Spanned<Expr>],
    ) -> CodeGenResult<Self::Result>;

    fn visit_field_access(
        &mut self,
        object: &Spanned<Expr>,
        field: &Ident,
    ) -> CodeGenResult<Self::Result>;

    fn visit_index(
        &mut self,
        object: &Spanned<Expr>,
        index: &Spanned<Expr>,
    ) -> CodeGenResult<Self::Result>;

    fn visit_assign(
        &mut self,
        target: &Spanned<Expr>,
        value: &Spanned<Expr>,
    ) -> CodeGenResult<Self::Result>;

    fn visit_new(
        &mut self,
        type_expr: &TypeExpr,
        fields: &[(Ident, Spanned<Expr>)],
    ) -> CodeGenResult<Self::Result>;
}
