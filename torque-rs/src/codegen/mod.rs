//! Code Generation Traits for Torque
//!
//! This module defines traits for generating code from Torque AST.
//! Implement these traits to target different backends (JVM bytecode, Java source, etc.)

use crate::ast::*;
use crate::lexer::F64Wrapper;

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

    #[error("Internal error: {0}")]
    Internal(String),
}

// ============================================================================
// Core Code Generation Trait
// ============================================================================

/// Main trait for code generation backends.
///
/// The `Output` associated type represents what the backend produces
/// (e.g., bytecode bytes, source code string, IR nodes).
///
/// # Example
///
/// ```ignore
/// struct JavaBytecodeGen {
///     class_file: ClassFile,
/// }
///
/// impl CodeGen for JavaBytecodeGen {
///     type Output = Vec<u8>;
///     type TypeRepr = JvmType;
///     type ExprResult = ();  // Leaves value on stack
///
///     fn generate(&mut self, file: &SourceFile) -> CodeGenResult<Self::Output> {
///         for decl in &file.declarations {
///             self.visit_declaration(&decl.node)?;
///         }
///         Ok(self.class_file.to_bytes())
///     }
///     // ...
/// }
/// ```
pub trait CodeGen {
    /// The final output of code generation
    type Output;

    /// How types are represented in the target
    type TypeRepr;

    /// The result of generating an expression (e.g., a register, stack effect)
    type ExprResult;

    /// Generate code for an entire source file
    fn generate(&mut self, file: &SourceFile) -> CodeGenResult<Self::Output>;

    /// Generate code for a single declaration
    fn visit_declaration(&mut self, decl: &Declaration) -> CodeGenResult<()>;

    /// Generate code for a statement
    fn visit_statement(&mut self, stmt: &Statement) -> CodeGenResult<()>;

    /// Generate code for an expression
    fn visit_expr(&mut self, expr: &Expr) -> CodeGenResult<Self::ExprResult>;

    /// Resolve a Torque type to the target type representation
    fn resolve_type(&mut self, ty: &TypeExpr) -> CodeGenResult<Self::TypeRepr>;
}

// ============================================================================
// Visitor Traits for Specific Constructs
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

// ============================================================================
// Type Mapping Trait
// ============================================================================

/// Trait for mapping Torque types to target types
pub trait TypeMapper {
    /// The target type representation
    type TargetType;

    /// Map a Torque type expression to a target type
    fn map_type(&self, ty: &TypeExpr) -> CodeGenResult<Self::TargetType>;

    /// Check if a type is a subtype of another
    fn is_subtype(&self, sub: &Self::TargetType, sup: &Self::TargetType) -> bool;

    /// Get the common supertype of two types (for type inference)
    fn common_type(
        &self,
        a: &Self::TargetType,
        b: &Self::TargetType,
    ) -> Option<Self::TargetType>;
}

// ============================================================================
// Label/Control Flow Trait
// ============================================================================

/// Trait for handling Torque's label-based control flow
pub trait LabelHandler {
    /// The representation of a label target
    type LabelTarget;

    /// Declare a new label that can be jumped to
    fn declare_label(&mut self, name: &Ident, params: &[TypeExpr]) -> CodeGenResult<Self::LabelTarget>;

    /// Generate code to jump to a label
    fn emit_goto(&mut self, target: &Self::LabelTarget, args: &[Spanned<Expr>]) -> CodeGenResult<()>;

    /// Start generating code for a label's body
    fn begin_label_body(&mut self, target: &Self::LabelTarget) -> CodeGenResult<()>;

    /// Finish generating code for a label's body
    fn end_label_body(&mut self, target: &Self::LabelTarget) -> CodeGenResult<()>;
}

// ============================================================================
// Default Implementations
// ============================================================================

/// Dispatch declaration to specific visitor methods
pub fn dispatch_declaration<V: DeclVisitor>(
    visitor: &mut V,
    decl: &Declaration,
) -> CodeGenResult<V::Result> {
    match decl {
        Declaration::Namespace(d) => visitor.visit_namespace(d),
        Declaration::Type(d) => visitor.visit_type_decl(d),
        Declaration::Macro(d) => visitor.visit_macro(d),
        Declaration::Builtin(d) => visitor.visit_builtin(d),
        Declaration::Extern(d) => visitor.visit_extern(d),
        Declaration::Const(d) => visitor.visit_const(d),
        Declaration::Class(d) => visitor.visit_class(d),
        Declaration::Struct(d) => visitor.visit_struct(d),
    }
}

/// Dispatch statement to specific visitor methods
pub fn dispatch_statement<V: StmtVisitor>(
    visitor: &mut V,
    stmt: &Statement,
) -> CodeGenResult<V::Result> {
    match stmt {
        Statement::VarDecl { is_const, name, type_expr, init } => {
            visitor.visit_var_decl(*is_const, name, type_expr.as_ref(), init)
        }
        Statement::Expr(e) => visitor.visit_expr_stmt(e),
        Statement::Return(v) => visitor.visit_return(v.as_ref()),
        Statement::If { condition, then_branch, else_branch } => {
            visitor.visit_if(condition, then_branch, else_branch.as_ref())
        }
        Statement::While { condition, body } => visitor.visit_while(condition, body),
        Statement::For { init, condition, update, body } => {
            visitor.visit_for(
                init.as_ref().map(|b| b.as_ref()),
                condition.as_ref(),
                update.as_ref(),
                body,
            )
        }
        Statement::Typeswitch { value, cases } => visitor.visit_typeswitch(value, cases),
        Statement::Try { body, handlers } => visitor.visit_try_label(body, handlers),
        Statement::Goto { label, args } => visitor.visit_goto(label, args),
        Statement::Break => visitor.visit_break(),
        Statement::Continue => visitor.visit_continue(),
        Statement::Unreachable => visitor.visit_unreachable(),
        Statement::Block(b) => visitor.visit_block(b),
        Statement::Tail(e) => visitor.visit_expr_stmt(e), // Treat as expr for now
        Statement::Assert { condition, .. } => visitor.visit_expr_stmt(condition),
    }
}

/// Dispatch expression to specific visitor methods
pub fn dispatch_expr<V: ExprVisitor>(
    visitor: &mut V,
    expr: &Expr,
) -> CodeGenResult<V::Result> {
    match expr {
        Expr::Ident(id) => visitor.visit_ident(id),
        Expr::IntLiteral(v) => visitor.visit_int_literal(*v),
        Expr::FloatLiteral(v) => visitor.visit_float_literal(*v),
        Expr::StringLiteral(v) => visitor.visit_string_literal(v),
        Expr::BoolLiteral(v) => visitor.visit_bool_literal(*v),
        Expr::Binary { op, left, right } => visitor.visit_binary(*op, left, right),
        Expr::Unary { op, operand } => visitor.visit_unary(*op, operand),
        Expr::Ternary { condition, then_expr, else_expr } => {
            visitor.visit_ternary(condition, then_expr, else_expr)
        }
        Expr::Call { callee, type_args, args, otherwise } => {
            visitor.visit_call(callee, type_args, args, otherwise)
        }
        Expr::Intrinsic { name, type_args, args } => {
            visitor.visit_intrinsic(name, type_args, args)
        }
        Expr::FieldAccess { object, field } => visitor.visit_field_access(object, field),
        Expr::Index { object, index } => visitor.visit_index(object, index),
        Expr::Assign { target, value } => visitor.visit_assign(target, value),
        Expr::New { type_expr, fields } => visitor.visit_new(type_expr, fields),
        Expr::Paren(inner) => dispatch_expr(visitor, &inner.node),
        Expr::CompoundAssign { op, target, value } => {
            // Desugar: x += y => x = x + y
            visitor.visit_binary(*op, target, value)
        }
        Expr::Increment { operand, .. } => {
            // Simplified handling
            visitor.visit_unary(UnaryOp::Neg, operand) // Placeholder
        }
        Expr::As { expr, .. } => dispatch_expr(visitor, &expr.node),
        Expr::Is { expr, .. } => dispatch_expr(visitor, &expr.node),
        Expr::Convert { expr, .. } => dispatch_expr(visitor, &expr.node),
    }
}

// ============================================================================
// Utility: AST Walker
// ============================================================================

/// Walk the entire AST, calling the visitor at each node
pub fn walk_ast<V: DeclVisitor<Result = ()> + StmtVisitor<Result = ()>>(
    visitor: &mut V,
    file: &SourceFile,
) -> CodeGenResult<()> {
    for decl in &file.declarations {
        dispatch_declaration(visitor, &decl.node)?;
    }
    Ok(())
}
