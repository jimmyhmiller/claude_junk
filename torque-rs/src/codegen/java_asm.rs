//! Java ASM Backend
//!
//! Generates Java source code that uses the ASM library to emit JVM bytecode.
//! This mirrors V8's approach where Torque generates C++ CSA code.
//!
//! Pipeline:
//!   Torque (.tq) → Java (ASM calls) → javac → .class → Run at startup → Bytecode
//!
//! The generated Java code creates bytecode builders for JavaScript builtins.

use crate::ast::*;
use crate::codegen::{Backend, CodeGenError, CodeGenResult, JvmType};
use std::collections::HashMap;
use std::fmt::Write;

// ============================================================================
// Java ASM Backend Types
// ============================================================================

/// Represents a value in the generated ASM code
#[derive(Debug, Clone)]
pub enum AsmValue {
    /// Value is on the JVM stack (implicit)
    Stack,
    /// Value is in a local variable slot
    Local(u32),
    /// Value is a constant that can be inlined
    Constant(String),
    /// No value (void)
    Void,
}

/// Represents a label in ASM
#[derive(Debug, Clone)]
pub struct AsmLabel {
    pub name: String,
    pub java_var: String,  // The Java variable name for this Label object
}

/// Output from the Java ASM backend
#[derive(Debug, Clone)]
pub struct JavaAsmOutput {
    /// Map of class name to Java source code
    pub classes: HashMap<String, String>,
}

// ============================================================================
// Java ASM Backend
// ============================================================================

/// Backend that generates Java source using ASM library
pub struct JavaAsmBackend {
    /// Package name for generated classes
    package: String,
    /// Current class being generated
    current_class: String,
    /// Current method being generated
    current_method: Option<MethodBuilder>,
    /// All generated classes
    classes: HashMap<String, ClassBuilder>,
    /// Type aliases
    type_aliases: HashMap<String, JvmType>,
    /// Label counter for unique names
    label_counter: u32,
    /// Local variable counter
    local_counter: u32,
    /// Variable name to local slot mapping
    locals: HashMap<String, u32>,
    /// Current namespace path
    namespace_stack: Vec<String>,
    /// Known labels in current scope
    labels: HashMap<String, AsmLabel>,
}

/// Builder for a Java class
#[derive(Debug, Clone)]
struct ClassBuilder {
    name: String,
    package: String,
    imports: Vec<String>,
    fields: Vec<String>,
    methods: Vec<String>,
    static_init: Vec<String>,
}

/// Builder for a method that emits ASM code
#[derive(Debug, Clone)]
struct MethodBuilder {
    name: String,
    params: Vec<(String, JvmType)>,
    return_type: JvmType,
    body: Vec<String>,
    indent: usize,
}

impl JavaAsmBackend {
    pub fn new(package: &str) -> Self {
        Self {
            package: package.to_string(),
            current_class: String::new(),
            current_method: None,
            classes: HashMap::new(),
            type_aliases: HashMap::new(),
            label_counter: 0,
            local_counter: 0,
            locals: HashMap::new(),
            namespace_stack: Vec::new(),
            labels: HashMap::new(),
        }
    }

    /// Get or create a class builder
    fn get_or_create_class(&mut self, name: &str) -> &mut ClassBuilder {
        let package = self.package.clone();
        self.classes.entry(name.to_string()).or_insert_with(|| {
            ClassBuilder::new(name, &package)
        })
    }

    /// Generate a unique label name
    fn fresh_label(&mut self, prefix: &str) -> String {
        self.label_counter += 1;
        format!("{}_{}", prefix, self.label_counter)
    }

    /// Allocate a local variable slot
    fn alloc_local(&mut self, name: &str) -> u32 {
        let slot = self.local_counter;
        self.local_counter += 1;
        self.locals.insert(name.to_string(), slot);
        slot
    }

    /// Emit a line of code to the current method
    fn emit(&mut self, code: &str) {
        if let Some(ref mut method) = self.current_method {
            method.emit(code);
        }
    }

    /// Emit ASM instruction to push an int constant
    fn emit_iconst(&mut self, value: i64) {
        let code = match value {
            -1 => "mv.visitInsn(ICONST_M1);".to_string(),
            0 => "mv.visitInsn(ICONST_0);".to_string(),
            1 => "mv.visitInsn(ICONST_1);".to_string(),
            2 => "mv.visitInsn(ICONST_2);".to_string(),
            3 => "mv.visitInsn(ICONST_3);".to_string(),
            4 => "mv.visitInsn(ICONST_4);".to_string(),
            5 => "mv.visitInsn(ICONST_5);".to_string(),
            -128..=127 => format!("mv.visitIntInsn(BIPUSH, {});", value),
            -32768..=32767 => format!("mv.visitIntInsn(SIPUSH, {});", value),
            _ => format!("mv.visitLdcInsn({});", value),
        };
        self.emit(&code);
    }

    /// Emit ASM instruction to load a local variable
    fn emit_load(&mut self, slot: u32, ty: &JvmType) {
        let insn = match ty {
            JvmType::Int | JvmType::Boolean | JvmType::Byte | JvmType::Char | JvmType::Short => "ILOAD",
            JvmType::Long => "LLOAD",
            JvmType::Float => "FLOAD",
            JvmType::Double => "DLOAD",
            JvmType::Object(_) | JvmType::Array(_) => "ALOAD",
            JvmType::Void => return,
        };
        self.emit(&format!("mv.visitVarInsn({}, {});", insn, slot));
    }

    /// Emit ASM instruction to store to a local variable
    fn emit_store(&mut self, slot: u32, ty: &JvmType) {
        let insn = match ty {
            JvmType::Int | JvmType::Boolean | JvmType::Byte | JvmType::Char | JvmType::Short => "ISTORE",
            JvmType::Long => "LSTORE",
            JvmType::Float => "FSTORE",
            JvmType::Double => "DSTORE",
            JvmType::Object(_) | JvmType::Array(_) => "ASTORE",
            JvmType::Void => return,
        };
        self.emit(&format!("mv.visitVarInsn({}, {});", insn, slot));
    }

    /// Map a Torque type to JVM type
    fn torque_to_jvm(&self, ty: &TypeExpr) -> CodeGenResult<JvmType> {
        match ty {
            TypeExpr::Named(ident) => {
                let name = ident.name.as_str();
                // Check aliases first
                if let Some(alias) = self.type_aliases.get(name) {
                    return Ok(alias.clone());
                }
                // Then check built-in mappings
                JvmType::from_torque(name).ok_or_else(|| {
                    CodeGenError::UnknownType(name.to_string())
                })
            }
            TypeExpr::Generic { name, args } => {
                // Handle generic types like FixedArray<T>
                let base_name = name.name.as_str();
                match base_name {
                    "FixedArray" | "ArrayList" => {
                        if let Some(elem) = args.first() {
                            let elem_type = self.torque_to_jvm(elem)?;
                            Ok(JvmType::Array(Box::new(elem_type)))
                        } else {
                            Ok(JvmType::Array(Box::new(JvmType::Object("java/lang/Object".to_string()))))
                        }
                    }
                    _ => {
                        // Default to Object for unknown generics
                        Ok(JvmType::Object(format!("js/runtime/{}", base_name)))
                    }
                }
            }
            TypeExpr::Union(_types) => {
                // Union types become Object in JVM
                // But we keep track for runtime checks
                Ok(JvmType::Object("java/lang/Object".to_string()))
            }
            TypeExpr::Function { params: _, return_type: _ } => {
                // Function types map to a functional interface
                Ok(JvmType::Object("java/util/function/Function".to_string()))
            }
            TypeExpr::Reference(inner) => {
                // References are just the inner type in JVM
                self.torque_to_jvm(inner)
            }
        }
    }

    /// Get the Java type name for a JVM type
    fn jvm_to_java(&self, ty: &JvmType) -> String {
        match ty {
            JvmType::Void => "void".to_string(),
            JvmType::Boolean => "boolean".to_string(),
            JvmType::Byte => "byte".to_string(),
            JvmType::Char => "char".to_string(),
            JvmType::Short => "short".to_string(),
            JvmType::Int => "int".to_string(),
            JvmType::Long => "long".to_string(),
            JvmType::Float => "float".to_string(),
            JvmType::Double => "double".to_string(),
            JvmType::Object(name) => {
                // Convert internal name to Java name
                name.replace('/', ".")
            }
            JvmType::Array(elem) => {
                format!("{}[]", self.jvm_to_java(elem))
            }
        }
    }

    /// Generate binary operation ASM code
    fn emit_binary_op(&mut self, op: BinaryOp, ty: &JvmType) {
        let insn = match (op, ty) {
            // Integer operations
            (BinaryOp::Add, JvmType::Int) => "IADD",
            (BinaryOp::Sub, JvmType::Int) => "ISUB",
            (BinaryOp::Mul, JvmType::Int) => "IMUL",
            (BinaryOp::Div, JvmType::Int) => "IDIV",
            (BinaryOp::Mod, JvmType::Int) => "IREM",
            (BinaryOp::BitAnd, JvmType::Int) => "IAND",
            (BinaryOp::BitOr, JvmType::Int) => "IOR",
            (BinaryOp::BitXor, JvmType::Int) => "IXOR",
            (BinaryOp::Shl, JvmType::Int) => "ISHL",
            (BinaryOp::Shr, JvmType::Int) => "ISHR",
            (BinaryOp::Ushr, JvmType::Int) => "IUSHR",

            // Long operations
            (BinaryOp::Add, JvmType::Long) => "LADD",
            (BinaryOp::Sub, JvmType::Long) => "LSUB",
            (BinaryOp::Mul, JvmType::Long) => "LMUL",
            (BinaryOp::Div, JvmType::Long) => "LDIV",
            (BinaryOp::Mod, JvmType::Long) => "LREM",
            (BinaryOp::BitAnd, JvmType::Long) => "LAND",
            (BinaryOp::BitOr, JvmType::Long) => "LOR",
            (BinaryOp::BitXor, JvmType::Long) => "LXOR",
            (BinaryOp::Shl, JvmType::Long) => "LSHL",
            (BinaryOp::Shr, JvmType::Long) => "LSHR",
            (BinaryOp::Ushr, JvmType::Long) => "LUSHR",

            // Float operations
            (BinaryOp::Add, JvmType::Float) => "FADD",
            (BinaryOp::Sub, JvmType::Float) => "FSUB",
            (BinaryOp::Mul, JvmType::Float) => "FMUL",
            (BinaryOp::Div, JvmType::Float) => "FDIV",
            (BinaryOp::Mod, JvmType::Float) => "FREM",

            // Double operations
            (BinaryOp::Add, JvmType::Double) => "DADD",
            (BinaryOp::Sub, JvmType::Double) => "DSUB",
            (BinaryOp::Mul, JvmType::Double) => "DMUL",
            (BinaryOp::Div, JvmType::Double) => "DDIV",
            (BinaryOp::Mod, JvmType::Double) => "DREM",

            // Comparisons need special handling
            (BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge, _) => {
                self.emit_comparison(op, ty);
                return;
            }

            // Logical operators
            (BinaryOp::And | BinaryOp::Or, _) => {
                // These need short-circuit evaluation
                return;
            }

            _ => {
                self.emit(&format!("// TODO: {} for {:?}", op_to_string(op), ty));
                return;
            }
        };
        self.emit(&format!("mv.visitInsn({});", insn));
    }

    /// Emit comparison operations
    fn emit_comparison(&mut self, op: BinaryOp, ty: &JvmType) {
        let true_label = self.fresh_label("cmp_true");
        let end_label = self.fresh_label("cmp_end");

        // Declare labels
        self.emit(&format!("Label {} = new Label();", true_label));
        self.emit(&format!("Label {} = new Label();", end_label));

        // For objects, use equals() or reference comparison
        if matches!(ty, JvmType::Object(_) | JvmType::Array(_)) {
            match op {
                BinaryOp::Eq => {
                    self.emit(&format!("mv.visitJumpInsn(IF_ACMPEQ, {});", true_label));
                }
                BinaryOp::Ne => {
                    self.emit(&format!("mv.visitJumpInsn(IF_ACMPNE, {});", true_label));
                }
                _ => {
                    self.emit("// Object comparison requires runtime support");
                }
            }
        } else {
            // Numeric comparison
            let jump_insn = match (op, ty) {
                (BinaryOp::Eq, JvmType::Int) => "IF_ICMPEQ",
                (BinaryOp::Ne, JvmType::Int) => "IF_ICMPNE",
                (BinaryOp::Lt, JvmType::Int) => "IF_ICMPLT",
                (BinaryOp::Le, JvmType::Int) => "IF_ICMPLE",
                (BinaryOp::Gt, JvmType::Int) => "IF_ICMPGT",
                (BinaryOp::Ge, JvmType::Int) => "IF_ICMPGE",

                // Long/float/double need LCMP/FCMP/DCMP first
                (BinaryOp::Eq, JvmType::Long) => {
                    self.emit("mv.visitInsn(LCMP);");
                    "IFEQ"
                }
                (BinaryOp::Ne, JvmType::Long) => {
                    self.emit("mv.visitInsn(LCMP);");
                    "IFNE"
                }
                (BinaryOp::Lt, JvmType::Long) => {
                    self.emit("mv.visitInsn(LCMP);");
                    "IFLT"
                }
                (BinaryOp::Le, JvmType::Long) => {
                    self.emit("mv.visitInsn(LCMP);");
                    "IFLE"
                }
                (BinaryOp::Gt, JvmType::Long) => {
                    self.emit("mv.visitInsn(LCMP);");
                    "IFGT"
                }
                (BinaryOp::Ge, JvmType::Long) => {
                    self.emit("mv.visitInsn(LCMP);");
                    "IFGE"
                }

                // Double comparisons
                (BinaryOp::Eq, JvmType::Double) => {
                    self.emit("mv.visitInsn(DCMPL);");
                    "IFEQ"
                }
                (BinaryOp::Ne, JvmType::Double) => {
                    self.emit("mv.visitInsn(DCMPL);");
                    "IFNE"
                }
                (BinaryOp::Lt, JvmType::Double) => {
                    self.emit("mv.visitInsn(DCMPL);");
                    "IFLT"
                }
                (BinaryOp::Le, JvmType::Double) => {
                    self.emit("mv.visitInsn(DCMPL);");
                    "IFLE"
                }
                (BinaryOp::Gt, JvmType::Double) => {
                    self.emit("mv.visitInsn(DCMPG);");
                    "IFGT"
                }
                (BinaryOp::Ge, JvmType::Double) => {
                    self.emit("mv.visitInsn(DCMPG);");
                    "IFGE"
                }

                _ => "IFEQ", // fallback
            };
            self.emit(&format!("mv.visitJumpInsn({}, {});", jump_insn, true_label));
        }

        // Push false, jump to end
        self.emit("mv.visitInsn(ICONST_0);");
        self.emit(&format!("mv.visitJumpInsn(GOTO, {});", end_label));

        // True label: push true
        self.emit(&format!("mv.visitLabel({});", true_label));
        self.emit("mv.visitInsn(ICONST_1);");

        // End label
        self.emit(&format!("mv.visitLabel({});", end_label));
    }

    /// Emit code for statements in a block
    fn emit_block(&mut self, block: &Block) -> CodeGenResult<()> {
        for stmt in &block.statements {
            self.emit_statement(&stmt.node)?;
        }
        Ok(())
    }

    /// Emit code for a single statement
    fn emit_statement(&mut self, stmt: &Statement) -> CodeGenResult<()> {
        match stmt {
            Statement::VarDecl { is_const, name, type_expr, init } => {
                self.emit_var_decl(*is_const, name, type_expr.as_ref(), init)?;
            }
            Statement::Expr(e) => {
                self.emit_expr(e)?;
                // Pop unused value if not void
                self.emit("// pop if non-void expression statement");
            }
            Statement::Return(v) => {
                self.emit_return(v.as_ref())?;
            }
            Statement::If { condition, then_branch, else_branch } => {
                self.emit_if(condition, then_branch, else_branch.as_ref())?;
            }
            Statement::While { condition, body } => {
                self.emit_while(condition, body)?;
            }
            Statement::For { init, condition, update, body } => {
                self.emit_for(
                    init.as_ref().map(|b| b.as_ref()),
                    condition.as_ref(),
                    update.as_ref(),
                    body,
                )?;
            }
            Statement::Typeswitch { value, cases } => {
                self.emit_typeswitch(value, cases)?;
            }
            Statement::Try { body, handlers } => {
                self.emit_try(body, handlers)?;
            }
            Statement::Goto { label, args } => {
                self.emit_goto(label, args)?;
            }
            Statement::Break => {
                self.emit_break()?;
            }
            Statement::Continue => {
                self.emit_continue()?;
            }
            Statement::Unreachable => {
                self.emit_unreachable()?;
            }
            Statement::Assert { kind, condition } => {
                self.emit_assert(*kind, condition)?;
            }
            Statement::Block(b) => {
                self.emit_block(b)?;
            }
            Statement::Tail(e) => {
                self.emit_expr(e)?;
                self.emit_return(Some(e))?;
            }
        }
        Ok(())
    }
}

impl ClassBuilder {
    fn new(name: &str, package: &str) -> Self {
        Self {
            name: name.to_string(),
            package: package.to_string(),
            imports: vec![
                "org.objectweb.asm.*".to_string(),
                "org.objectweb.asm.commons.*".to_string(),
                "static org.objectweb.asm.Opcodes.*".to_string(),
            ],
            fields: Vec::new(),
            methods: Vec::new(),
            static_init: Vec::new(),
        }
    }

    fn to_java(&self) -> String {
        let mut out = String::new();

        // Package declaration
        if !self.package.is_empty() {
            writeln!(out, "package {};", self.package).unwrap();
            writeln!(out).unwrap();
        }

        // Imports
        for import in &self.imports {
            writeln!(out, "import {};", import).unwrap();
        }
        writeln!(out).unwrap();

        // Class declaration
        writeln!(out, "/**").unwrap();
        writeln!(out, " * Generated from Torque source.").unwrap();
        writeln!(out, " * This class builds JVM bytecode for JavaScript builtins.").unwrap();
        writeln!(out, " */").unwrap();
        writeln!(out, "public class {} {{", self.name).unwrap();

        // Fields
        for field in &self.fields {
            writeln!(out, "    {}", field).unwrap();
        }
        if !self.fields.is_empty() {
            writeln!(out).unwrap();
        }

        // Static initializer
        if !self.static_init.is_empty() {
            writeln!(out, "    static {{").unwrap();
            for line in &self.static_init {
                writeln!(out, "        {}", line).unwrap();
            }
            writeln!(out, "    }}").unwrap();
            writeln!(out).unwrap();
        }

        // Methods
        for method in &self.methods {
            writeln!(out, "{}", method).unwrap();
        }

        writeln!(out, "}}").unwrap();
        out
    }
}

impl MethodBuilder {
    fn new(name: &str, params: Vec<(String, JvmType)>, return_type: JvmType) -> Self {
        Self {
            name: name.to_string(),
            params,
            return_type,
            body: Vec::new(),
            indent: 2,
        }
    }

    fn emit(&mut self, code: &str) {
        let indent = "    ".repeat(self.indent);
        self.body.push(format!("{}{}", indent, code));
    }

    fn indent(&mut self) {
        self.indent += 1;
    }

    fn dedent(&mut self) {
        if self.indent > 0 {
            self.indent -= 1;
        }
    }

    fn to_java(&self, backend: &JavaAsmBackend) -> String {
        let mut out = String::new();

        // Build parameter list
        let params: Vec<String> = self.params.iter()
            .map(|(name, ty)| format!("{} {}", backend.jvm_to_java(ty), name))
            .collect();

        // Method signature
        writeln!(out, "    /**").unwrap();
        writeln!(out, "     * Generates bytecode for {} builtin.", self.name).unwrap();
        writeln!(out, "     * @param cw The ClassWriter to emit bytecode to").unwrap();
        writeln!(out, "     */").unwrap();
        writeln!(out, "    public static void build_{}(ClassWriter cw) {{", self.name).unwrap();

        // Create MethodVisitor
        let return_desc = self.return_type.descriptor();
        let params_desc: String = self.params.iter()
            .map(|(_, ty)| ty.descriptor())
            .collect();
        writeln!(out, "        MethodVisitor mv = cw.visitMethod(").unwrap();
        writeln!(out, "            ACC_PUBLIC | ACC_STATIC,").unwrap();
        writeln!(out, "            \"{}\",", self.name).unwrap();
        writeln!(out, "            \"({}){}\",", params_desc, return_desc).unwrap();
        writeln!(out, "            null,").unwrap();
        writeln!(out, "            null);").unwrap();
        writeln!(out, "        mv.visitCode();").unwrap();
        writeln!(out).unwrap();

        // Method body
        for line in &self.body {
            writeln!(out, "{}", line).unwrap();
        }

        // End method
        writeln!(out).unwrap();
        writeln!(out, "        mv.visitMaxs(-1, -1);  // Auto-computed").unwrap();
        writeln!(out, "        mv.visitEnd();").unwrap();
        writeln!(out, "    }}").unwrap();

        out
    }
}

// ============================================================================
// Backend Implementation
// ============================================================================

impl Backend for JavaAsmBackend {
    type Type = JvmType;
    type Value = AsmValue;
    type Label = AsmLabel;
    type Output = JavaAsmOutput;

    fn map_type(&mut self, ty: &TypeExpr) -> CodeGenResult<Self::Type> {
        self.torque_to_jvm(ty)
    }

    fn primitive_type(&self, name: &str) -> CodeGenResult<Self::Type> {
        JvmType::from_torque(name).ok_or_else(|| CodeGenError::UnknownType(name.to_string()))
    }

    fn union_type(&mut self, _types: &[Self::Type]) -> CodeGenResult<Self::Type> {
        // Union types become Object in JVM
        Ok(JvmType::Object("java/lang/Object".to_string()))
    }

    fn function_type(
        &mut self,
        _params: &[Self::Type],
        _return_type: &Self::Type,
    ) -> CodeGenResult<Self::Type> {
        // Function types map to functional interface
        Ok(JvmType::Object("java/util/function/Function".to_string()))
    }

    fn emit_namespace(&mut self, decl: &NamespaceDecl) -> CodeGenResult<()> {
        self.namespace_stack.push(decl.name.name.to_string());

        // Create a class for this namespace
        let class_name = decl.name.name.to_string();
        self.current_class = class_name.clone();
        self.get_or_create_class(&class_name);

        Ok(())
    }

    fn emit_type_decl(&mut self, decl: &TypeDecl) -> CodeGenResult<()> {
        // Register type alias if it has a generates clause
        if let Some(ref generates) = decl.generates {
            // Parse the generates clause to determine JVM type
            let jvm_type = if generates.contains("TNode<Smi>") {
                JvmType::Object("js/runtime/Smi".to_string())
            } else if generates.contains("TNode<") {
                // Extract type from TNode<Type>
                let inner = generates
                    .trim_start_matches("TNode<")
                    .trim_end_matches('>');
                JvmType::from_torque(inner)
                    .unwrap_or(JvmType::Object(format!("js/runtime/{}", inner)))
            } else {
                JvmType::Object("java/lang/Object".to_string())
            };
            self.type_aliases.insert(decl.name.name.to_string(), jvm_type);
        }
        Ok(())
    }

    fn emit_macro(&mut self, decl: &MacroDecl) -> CodeGenResult<()> {
        // Macros become static methods that emit bytecode
        let return_type = decl.return_type
            .as_ref()
            .map(|t| self.torque_to_jvm(t))
            .transpose()?
            .unwrap_or(JvmType::Void);

        let params: Vec<(String, JvmType)> = decl.params
            .iter()
            .filter_map(|p| {
                let ty = self.torque_to_jvm(&p.type_expr).ok()?;
                Some((p.name.name.to_string(), ty))
            })
            .collect();

        // Reset local state
        self.local_counter = 0;
        self.locals.clear();
        self.labels.clear();

        // Allocate slots for parameters
        for (name, _ty) in &params {
            let _slot = self.alloc_local(name);
        }

        // Create method builder
        let mut method = MethodBuilder::new(&decl.name.name, params.clone(), return_type);

        // Add labels for any declared labels
        for label_decl in &decl.labels {
            let label_name = label_decl.name.name.to_string();
            let java_var = self.fresh_label(&label_name);
            method.emit(&format!("Label {} = new Label();", java_var));
            self.labels.insert(label_name.clone(), AsmLabel {
                name: label_name,
                java_var,
            });
        }

        self.current_method = Some(method);

        // Generate body
        if let Some(ref body) = decl.body {
            self.emit_block(body)?;
        }

        // Finalize method
        if let Some(method) = self.current_method.take() {
            let java_code = method.to_java(self);
            let current_class = self.current_class.clone();
            let class = self.get_or_create_class(&current_class);
            class.methods.push(java_code);
        }

        Ok(())
    }

    fn emit_builtin(&mut self, decl: &BuiltinDecl) -> CodeGenResult<()> {
        // Builtins are like macros but may have different calling conventions
        let return_type = decl.return_type
            .as_ref()
            .map(|t| self.torque_to_jvm(t))
            .transpose()?
            .unwrap_or(JvmType::Void);

        let mut params: Vec<(String, JvmType)> = decl.params
            .iter()
            .filter_map(|p| {
                let ty = self.torque_to_jvm(&p.type_expr).ok()?;
                Some((p.name.name.to_string(), ty))
            })
            .collect();

        // JavaScript builtins get implicit receiver and context
        if decl.is_javascript {
            params.insert(0, ("receiver".to_string(), JvmType::Object("java/lang/Object".to_string())));
            params.insert(0, ("context".to_string(), JvmType::Object("js/runtime/Context".to_string())));
        }

        // Reset local state
        self.local_counter = 0;
        self.locals.clear();
        self.labels.clear();

        for (name, _) in &params {
            self.alloc_local(name);
        }

        let method = MethodBuilder::new(&decl.name.name, params, return_type);
        self.current_method = Some(method);

        if let Some(ref body) = decl.body {
            self.emit_block(body)?;
        }

        if let Some(method) = self.current_method.take() {
            let java_code = method.to_java(self);
            let current_class = self.current_class.clone();
            let class = self.get_or_create_class(&current_class);
            class.methods.push(java_code);
        }

        Ok(())
    }

    fn emit_extern(&mut self, decl: &ExternDecl) -> CodeGenResult<()> {
        // Extern declarations register external functions
        match decl {
            ExternDecl::Macro(_m) => {
                // Just register, don't generate body
                self.emit("// extern macro");
            }
            ExternDecl::Builtin(_b) => {
                self.emit("// extern builtin");
            }
            ExternDecl::Runtime(_r) => {
                // Runtime functions call into the JS runtime
                self.emit("// extern runtime");
            }
        }
        Ok(())
    }

    fn emit_const(&mut self, decl: &ConstDecl) -> CodeGenResult<()> {
        let ty = self.torque_to_jvm(&decl.type_expr)?;
        let java_type = self.jvm_to_java(&ty);
        let field_decl = format!(
            "public static final {} {} = /* value */;",
            java_type,
            decl.name.name
        );

        let current_class = self.current_class.clone();
        let class = self.get_or_create_class(&current_class);
        class.fields.push(field_decl);

        Ok(())
    }

    fn emit_class(&mut self, decl: &ClassDecl) -> CodeGenResult<()> {
        // Torque classes become Java classes that can build their own bytecode
        let class_name = decl.name.name.to_string();

        // Collect field info first
        let mut field_decls: Vec<String> = Vec::new();
        for field in &decl.fields {
            let ty = self.torque_to_jvm(&field.type_expr)?;
            let java_type = self.jvm_to_java(&ty);
            field_decls.push(format!(
                "// Field: {} {}",
                java_type,
                field.name.name
            ));
        }

        // Handle extends
        if let Some(ref parent) = decl.extends {
            let parent_ty = self.torque_to_jvm(parent)?;
            field_decls.push(format!(
                "// extends: {}",
                self.jvm_to_java(&parent_ty)
            ));
        }

        // Now get class and add fields
        let class = self.get_or_create_class(&class_name);
        for field_decl in field_decls {
            class.fields.push(field_decl);
        }

        Ok(())
    }

    fn emit_struct(&mut self, decl: &StructDecl) -> CodeGenResult<()> {
        // Structs are similar to classes but value types
        let class_name = format!("{}Struct", decl.name.name);

        // Collect field info first
        let mut field_decls: Vec<String> = Vec::new();
        for field in &decl.fields {
            let ty = self.torque_to_jvm(&field.type_expr)?;
            let java_type = self.jvm_to_java(&ty);
            field_decls.push(format!(
                "public {} {};",
                java_type,
                field.name.name
            ));
        }

        // Now get class and add fields
        let class = self.get_or_create_class(&class_name);
        for field_decl in field_decls {
            class.fields.push(field_decl);
        }

        Ok(())
    }

    fn emit_var_decl(
        &mut self,
        is_const: bool,
        name: &Ident,
        ty: Option<&TypeExpr>,
        init: &Spanned<Expr>,
    ) -> CodeGenResult<()> {
        let var_name = name.name.to_string();

        // Infer type from init expression if not specified
        let jvm_type = if let Some(t) = ty {
            self.torque_to_jvm(t)?
        } else {
            // Default to Object for now
            JvmType::Object("java/lang/Object".to_string())
        };

        // Allocate local slot
        let slot = self.alloc_local(&var_name);

        // Emit initialization
        self.emit(&format!("// {} {} = ...", if is_const { "const" } else { "let" }, var_name));
        self.emit_expr(init)?;
        self.emit_store(slot, &jvm_type);

        Ok(())
    }

    fn emit_return(&mut self, value: Option<&Spanned<Expr>>) -> CodeGenResult<()> {
        if let Some(expr) = value {
            self.emit_expr(expr)?;
            // Determine return type and emit appropriate return
            self.emit("mv.visitInsn(ARETURN);  // TODO: correct return type");
        } else {
            self.emit("mv.visitInsn(RETURN);");
        }
        Ok(())
    }

    fn emit_if(
        &mut self,
        condition: &Spanned<Expr>,
        then_branch: &Block,
        else_branch: Option<&Block>,
    ) -> CodeGenResult<()> {
        let else_label = self.fresh_label("else");
        let end_label = self.fresh_label("endif");

        self.emit(&format!("Label {} = new Label();", else_label));
        self.emit(&format!("Label {} = new Label();", end_label));

        // Evaluate condition
        self.emit_expr(condition)?;

        // Branch to else if false
        self.emit(&format!("mv.visitJumpInsn(IFEQ, {});", else_label));

        // Then branch
        self.emit_block(then_branch)?;
        self.emit(&format!("mv.visitJumpInsn(GOTO, {});", end_label));

        // Else branch
        self.emit(&format!("mv.visitLabel({});", else_label));
        if let Some(else_block) = else_branch {
            self.emit_block(else_block)?;
        }

        self.emit(&format!("mv.visitLabel({});", end_label));
        Ok(())
    }

    fn emit_while(&mut self, condition: &Spanned<Expr>, body: &Block) -> CodeGenResult<()> {
        let loop_start = self.fresh_label("while_start");
        let loop_end = self.fresh_label("while_end");

        self.emit(&format!("Label {} = new Label();", loop_start));
        self.emit(&format!("Label {} = new Label();", loop_end));

        // Loop start
        self.emit(&format!("mv.visitLabel({});", loop_start));

        // Condition
        self.emit_expr(condition)?;
        self.emit(&format!("mv.visitJumpInsn(IFEQ, {});", loop_end));

        // Body
        self.emit_block(body)?;
        self.emit(&format!("mv.visitJumpInsn(GOTO, {});", loop_start));

        // End
        self.emit(&format!("mv.visitLabel({});", loop_end));
        Ok(())
    }

    fn emit_for(
        &mut self,
        init: Option<&Spanned<Statement>>,
        condition: Option<&Spanned<Expr>>,
        update: Option<&Spanned<Expr>>,
        body: &Block,
    ) -> CodeGenResult<()> {
        let loop_start = self.fresh_label("for_start");
        let loop_end = self.fresh_label("for_end");
        let loop_update = self.fresh_label("for_update");

        self.emit(&format!("Label {} = new Label();", loop_start));
        self.emit(&format!("Label {} = new Label();", loop_end));
        self.emit(&format!("Label {} = new Label();", loop_update));

        // Init
        if let Some(stmt) = init {
            self.emit_statement(&stmt.node)?;
        }

        // Loop start
        self.emit(&format!("mv.visitLabel({});", loop_start));

        // Condition
        if let Some(cond) = condition {
            self.emit_expr(cond)?;
            self.emit(&format!("mv.visitJumpInsn(IFEQ, {});", loop_end));
        }

        // Body
        self.emit_block(body)?;

        // Update
        self.emit(&format!("mv.visitLabel({});", loop_update));
        if let Some(upd) = update {
            self.emit_expr(upd)?;
        }
        self.emit(&format!("mv.visitJumpInsn(GOTO, {});", loop_start));

        // End
        self.emit(&format!("mv.visitLabel({});", loop_end));
        Ok(())
    }

    fn emit_typeswitch(
        &mut self,
        value: &Spanned<Expr>,
        cases: &[TypeswitchCase],
    ) -> CodeGenResult<()> {
        let end_label = self.fresh_label("typeswitch_end");
        self.emit(&format!("Label {} = new Label();", end_label));

        // Evaluate value and store in local
        self.emit_expr(value)?;
        let value_slot = self.local_counter;
        self.local_counter += 1;
        self.emit(&format!("mv.visitVarInsn(ASTORE, {});", value_slot));

        // Generate instanceof checks for each case
        for (i, case) in cases.iter().enumerate() {
            let case_label = self.fresh_label(&format!("case_{}", i));
            let next_label = self.fresh_label(&format!("next_{}", i));

            self.emit(&format!("Label {} = new Label();", next_label));

            let jvm_type = self.torque_to_jvm(&case.type_expr)?;
            if let JvmType::Object(ref class_name) = jvm_type {
                // Load value and check instanceof
                self.emit(&format!("mv.visitVarInsn(ALOAD, {});", value_slot));
                self.emit(&format!("mv.visitTypeInsn(INSTANCEOF, \"{}\");", class_name));
                self.emit(&format!("mv.visitJumpInsn(IFEQ, {});", next_label));

                // Cast and store as binding
                let binding_slot = self.alloc_local(&case.binding.name);
                self.emit(&format!("mv.visitVarInsn(ALOAD, {});", value_slot));
                self.emit(&format!("mv.visitTypeInsn(CHECKCAST, \"{}\");", class_name));
                self.emit(&format!("mv.visitVarInsn(ASTORE, {});", binding_slot));

                // Case body
                self.emit_block(&case.body)?;
                self.emit(&format!("mv.visitJumpInsn(GOTO, {});", end_label));
            }

            self.emit(&format!("mv.visitLabel({});", next_label));
        }

        self.emit(&format!("mv.visitLabel({});", end_label));
        Ok(())
    }

    fn emit_try(
        &mut self,
        body: &Block,
        handlers: &[LabelBlock],
    ) -> CodeGenResult<()> {
        let try_start = self.fresh_label("try_start");
        let try_end = self.fresh_label("try_end");
        let after_handlers = self.fresh_label("after_handlers");

        self.emit(&format!("Label {} = new Label();", try_start));
        self.emit(&format!("Label {} = new Label();", try_end));
        self.emit(&format!("Label {} = new Label();", after_handlers));

        // Register exception handlers
        for handler in handlers {
            let handler_label = self.fresh_label(&handler.name.name);
            self.emit(&format!("Label {} = new Label();", handler_label));
            self.labels.insert(handler.name.name.to_string(), AsmLabel {
                name: handler.name.name.to_string(),
                java_var: handler_label.clone(),
            });

            // Register try-catch
            self.emit(&format!(
                "mv.visitTryCatchBlock({}, {}, {}, \"js/runtime/LabelException\");",
                try_start, try_end, handler_label
            ));
        }

        // Try body
        self.emit(&format!("mv.visitLabel({});", try_start));
        self.emit_block(body)?;
        self.emit(&format!("mv.visitLabel({});", try_end));
        self.emit(&format!("mv.visitJumpInsn(GOTO, {});", after_handlers));

        // Handler bodies
        for handler in handlers {
            if let Some(label) = self.labels.get(&handler.name.name.to_string()) {
                self.emit(&format!("mv.visitLabel({});", label.java_var));
                // Extract parameters from exception if any
                for (i, param) in handler.params.iter().enumerate() {
                    let slot = self.alloc_local(&param.name.name);
                    // Pop exception and get parameter
                    self.emit(&format!("// extract param {} from exception", i));
                }
                self.emit_block(&handler.body)?;
            }
        }

        self.emit(&format!("mv.visitLabel({});", after_handlers));
        Ok(())
    }

    fn emit_goto(&mut self, label: &Ident, args: &[Spanned<Expr>]) -> CodeGenResult<()> {
        // Evaluate arguments
        for arg in args {
            self.emit_expr(arg)?;
        }

        // In JVM, gotos to labels become throwing exceptions
        if let Some(target) = self.labels.get(&label.name.to_string()) {
            self.emit(&format!(
                "// goto {} - throw LabelException",
                label.name
            ));
            self.emit("mv.visitTypeInsn(NEW, \"js/runtime/LabelException\");");
            self.emit("mv.visitInsn(DUP);");
            self.emit(&format!(
                "mv.visitLdcInsn(\"{}\");",
                label.name
            ));
            self.emit("mv.visitMethodInsn(INVOKESPECIAL, \"js/runtime/LabelException\", \"<init>\", \"(Ljava/lang/String;)V\", false);");
            self.emit("mv.visitInsn(ATHROW);");
        } else {
            return Err(CodeGenError::UnknownLabel(label.name.to_string()));
        }
        Ok(())
    }

    fn emit_break(&mut self) -> CodeGenResult<()> {
        self.emit("// break - jump to loop end");
        // Would need loop context to implement properly
        Ok(())
    }

    fn emit_continue(&mut self) -> CodeGenResult<()> {
        self.emit("// continue - jump to loop update/start");
        Ok(())
    }

    fn emit_unreachable(&mut self) -> CodeGenResult<()> {
        self.emit("// unreachable");
        self.emit("mv.visitTypeInsn(NEW, \"java/lang/AssertionError\");");
        self.emit("mv.visitInsn(DUP);");
        self.emit("mv.visitLdcInsn(\"unreachable\");");
        self.emit("mv.visitMethodInsn(INVOKESPECIAL, \"java/lang/AssertionError\", \"<init>\", \"(Ljava/lang/Object;)V\", false);");
        self.emit("mv.visitInsn(ATHROW);");
        Ok(())
    }

    fn emit_assert(&mut self, kind: AssertKind, condition: &Spanned<Expr>) -> CodeGenResult<()> {
        let pass_label = self.fresh_label("assert_pass");
        self.emit(&format!("Label {} = new Label();", pass_label));

        // Evaluate condition
        self.emit_expr(condition)?;
        self.emit(&format!("mv.visitJumpInsn(IFNE, {});", pass_label));

        // Assertion failed
        match kind {
            AssertKind::Dcheck => {
                self.emit("// dcheck failed");
            }
            AssertKind::Check => {
                self.emit("// check failed - throw");
                self.emit("mv.visitTypeInsn(NEW, \"java/lang/AssertionError\");");
                self.emit("mv.visitInsn(DUP);");
                self.emit("mv.visitMethodInsn(INVOKESPECIAL, \"java/lang/AssertionError\", \"<init>\", \"()V\", false);");
                self.emit("mv.visitInsn(ATHROW);");
            }
        }

        self.emit(&format!("mv.visitLabel({});", pass_label));
        Ok(())
    }

    fn emit_expr(&mut self, expr: &Spanned<Expr>) -> CodeGenResult<Self::Value> {
        match &expr.node {
            Expr::Ident(ident) => self.emit_ident(ident),
            Expr::IntLiteral(v) => {
                self.emit_iconst(*v);
                Ok(AsmValue::Stack)
            }
            Expr::FloatLiteral(v) => {
                self.emit(&format!("mv.visitLdcInsn({});", v.0));
                Ok(AsmValue::Stack)
            }
            Expr::StringLiteral(s) => {
                self.emit(&format!("mv.visitLdcInsn(\"{}\");", escape_java_string(s)));
                Ok(AsmValue::Stack)
            }
            Expr::BoolLiteral(b) => {
                if *b {
                    self.emit("mv.visitInsn(ICONST_1);");
                } else {
                    self.emit("mv.visitInsn(ICONST_0);");
                }
                Ok(AsmValue::Stack)
            }
            Expr::Binary { op, left, right } => self.emit_binary(*op, left, right),
            Expr::Unary { op, operand } => self.emit_unary(*op, operand),
            Expr::Ternary { condition, then_expr, else_expr } => {
                let else_label = self.fresh_label("ternary_else");
                let end_label = self.fresh_label("ternary_end");

                self.emit(&format!("Label {} = new Label();", else_label));
                self.emit(&format!("Label {} = new Label();", end_label));

                self.emit_expr(condition)?;
                self.emit(&format!("mv.visitJumpInsn(IFEQ, {});", else_label));
                self.emit_expr(then_expr)?;
                self.emit(&format!("mv.visitJumpInsn(GOTO, {});", end_label));
                self.emit(&format!("mv.visitLabel({});", else_label));
                self.emit_expr(else_expr)?;
                self.emit(&format!("mv.visitLabel({});", end_label));

                Ok(AsmValue::Stack)
            }
            Expr::Call { callee, type_args, args, otherwise } => {
                self.emit_call(callee, type_args, args, otherwise)
            }
            Expr::Intrinsic { name, type_args, args } => {
                // Intrinsics are special runtime calls
                self.emit(&format!("// intrinsic %{}", name.name));
                for arg in args {
                    self.emit_expr(arg)?;
                }
                self.emit(&format!(
                    "mv.visitMethodInsn(INVOKESTATIC, \"js/runtime/Intrinsics\", \"{}\", \"(...)Ljava/lang/Object;\", false);",
                    name.name
                ));
                Ok(AsmValue::Stack)
            }
            Expr::FieldAccess { object, field } => self.emit_field_access(object, field),
            Expr::Index { object, index } => self.emit_index(object, index),
            Expr::Assign { target, value } => self.emit_assign(target, value),
            Expr::CompoundAssign { op, target, value } => {
                // x += y => x = x + y
                self.emit_expr(target)?;
                self.emit_expr(value)?;
                self.emit_binary_op(*op, &JvmType::Int); // TODO: proper type
                // Now store back
                self.emit("// store compound assignment result");
                Ok(AsmValue::Stack)
            }
            Expr::Increment { operand, is_prefix, is_decrement } => {
                // TODO: proper increment handling
                self.emit_expr(operand)?;
                if *is_decrement {
                    self.emit("mv.visitInsn(ICONST_1);");
                    self.emit("mv.visitInsn(ISUB);");
                } else {
                    self.emit("mv.visitInsn(ICONST_1);");
                    self.emit("mv.visitInsn(IADD);");
                }
                Ok(AsmValue::Stack)
            }
            Expr::New { type_expr, fields } => self.emit_new(type_expr, fields),
            Expr::As { expr, type_expr } => {
                self.emit_expr(expr)?;
                let ty = self.torque_to_jvm(type_expr)?;
                if let JvmType::Object(ref class) = ty {
                    self.emit(&format!("mv.visitTypeInsn(CHECKCAST, \"{}\");", class));
                }
                Ok(AsmValue::Stack)
            }
            Expr::Is { type_expr, expr } => {
                self.emit_expr(expr)?;
                let ty = self.torque_to_jvm(type_expr)?;
                if let JvmType::Object(ref class) = ty {
                    self.emit(&format!("mv.visitTypeInsn(INSTANCEOF, \"{}\");", class));
                }
                Ok(AsmValue::Stack)
            }
            Expr::Convert { kind, type_expr, expr } => {
                self.emit_expr(expr)?;
                // Convert/cast operations
                let ty = self.torque_to_jvm(type_expr)?;
                self.emit(&format!("// convert to {:?}", ty));
                Ok(AsmValue::Stack)
            }
            Expr::Paren(inner) => self.emit_expr(inner),
        }
    }

    fn emit_ident(&mut self, ident: &Ident) -> CodeGenResult<Self::Value> {
        let name = ident.name.to_string();
        if let Some(&slot) = self.locals.get(&name) {
            // Load from local variable
            self.emit(&format!("mv.visitVarInsn(ALOAD, {});  // {}", slot, name));
            Ok(AsmValue::Local(slot))
        } else {
            // Might be a global/constant
            self.emit(&format!("// load identifier: {}", name));
            Ok(AsmValue::Constant(name))
        }
    }

    fn emit_literal(&mut self, lit: &Expr) -> CodeGenResult<Self::Value> {
        match lit {
            Expr::IntLiteral(v) => {
                self.emit_iconst(*v);
                Ok(AsmValue::Stack)
            }
            Expr::FloatLiteral(v) => {
                self.emit(&format!("mv.visitLdcInsn({});", v.0));
                Ok(AsmValue::Stack)
            }
            Expr::StringLiteral(s) => {
                self.emit(&format!("mv.visitLdcInsn(\"{}\");", escape_java_string(s)));
                Ok(AsmValue::Stack)
            }
            Expr::BoolLiteral(b) => {
                self.emit(if *b { "mv.visitInsn(ICONST_1);" } else { "mv.visitInsn(ICONST_0);" });
                Ok(AsmValue::Stack)
            }
            _ => Err(CodeGenError::Internal("Not a literal".to_string())),
        }
    }

    fn emit_binary(
        &mut self,
        op: BinaryOp,
        left: &Spanned<Expr>,
        right: &Spanned<Expr>,
    ) -> CodeGenResult<Self::Value> {
        // Short-circuit for && and ||
        if matches!(op, BinaryOp::And | BinaryOp::Or) {
            let short_circuit = self.fresh_label("short_circuit");
            let end = self.fresh_label("end");

            self.emit(&format!("Label {} = new Label();", short_circuit));
            self.emit(&format!("Label {} = new Label();", end));

            self.emit_expr(left)?;
            if op == BinaryOp::And {
                self.emit(&format!("mv.visitJumpInsn(IFEQ, {});", short_circuit));
            } else {
                self.emit(&format!("mv.visitJumpInsn(IFNE, {});", short_circuit));
            }

            self.emit_expr(right)?;
            self.emit(&format!("mv.visitJumpInsn(GOTO, {});", end));

            self.emit(&format!("mv.visitLabel({});", short_circuit));
            if op == BinaryOp::And {
                self.emit("mv.visitInsn(ICONST_0);");
            } else {
                self.emit("mv.visitInsn(ICONST_1);");
            }

            self.emit(&format!("mv.visitLabel({});", end));
            return Ok(AsmValue::Stack);
        }

        // Normal binary ops
        self.emit_expr(left)?;
        self.emit_expr(right)?;
        self.emit_binary_op(op, &JvmType::Int); // TODO: infer type

        Ok(AsmValue::Stack)
    }

    fn emit_unary(&mut self, op: UnaryOp, operand: &Spanned<Expr>) -> CodeGenResult<Self::Value> {
        self.emit_expr(operand)?;
        match op {
            UnaryOp::Neg => self.emit("mv.visitInsn(INEG);"),
            UnaryOp::Not => {
                // !x => x == 0 ? 1 : 0
                let true_label = self.fresh_label("not_true");
                let end_label = self.fresh_label("not_end");
                self.emit(&format!("Label {} = new Label();", true_label));
                self.emit(&format!("Label {} = new Label();", end_label));
                self.emit(&format!("mv.visitJumpInsn(IFEQ, {});", true_label));
                self.emit("mv.visitInsn(ICONST_0);");
                self.emit(&format!("mv.visitJumpInsn(GOTO, {});", end_label));
                self.emit(&format!("mv.visitLabel({});", true_label));
                self.emit("mv.visitInsn(ICONST_1);");
                self.emit(&format!("mv.visitLabel({});", end_label));
            }
            UnaryOp::BitNot => {
                // ~x => x ^ -1
                self.emit("mv.visitInsn(ICONST_M1);");
                self.emit("mv.visitInsn(IXOR);");
            }
        }
        Ok(AsmValue::Stack)
    }

    fn emit_call(
        &mut self,
        callee: &Spanned<Expr>,
        type_args: &[TypeExpr],
        args: &[Spanned<Expr>],
        otherwise: &[Ident],
    ) -> CodeGenResult<Self::Value> {
        // Determine callee name
        let callee_name = match &callee.node {
            Expr::Ident(id) => id.name.to_string(),
            Expr::FieldAccess { object, field } => {
                // Method call: obj.method(args)
                self.emit_expr(object)?;
                field.name.to_string()
            }
            _ => "unknown".to_string(),
        };

        // Push arguments
        for arg in args {
            self.emit_expr(arg)?;
        }

        // Handle otherwise labels (for calls that can fail)
        if !otherwise.is_empty() {
            self.emit("// call with otherwise labels");
            // Would wrap in try-catch
        }

        // Emit call
        self.emit(&format!(
            "mv.visitMethodInsn(INVOKESTATIC, \"js/runtime/Builtins\", \"{}\", \"(...)Ljava/lang/Object;\", false);",
            callee_name
        ));

        Ok(AsmValue::Stack)
    }

    fn emit_field_access(
        &mut self,
        object: &Spanned<Expr>,
        field: &Ident,
    ) -> CodeGenResult<Self::Value> {
        self.emit_expr(object)?;
        self.emit(&format!(
            "mv.visitFieldInsn(GETFIELD, \"js/runtime/JSObject\", \"{}\", \"Ljava/lang/Object;\");",
            field.name
        ));
        Ok(AsmValue::Stack)
    }

    fn emit_index(
        &mut self,
        object: &Spanned<Expr>,
        index: &Spanned<Expr>,
    ) -> CodeGenResult<Self::Value> {
        self.emit_expr(object)?;
        self.emit_expr(index)?;
        self.emit("mv.visitInsn(AALOAD);");
        Ok(AsmValue::Stack)
    }

    fn emit_assign(
        &mut self,
        target: &Spanned<Expr>,
        value: &Spanned<Expr>,
    ) -> CodeGenResult<Self::Value> {
        match &target.node {
            Expr::Ident(ident) => {
                self.emit_expr(value)?;
                if let Some(&slot) = self.locals.get(&ident.name.to_string()) {
                    self.emit(&format!("mv.visitVarInsn(ASTORE, {});", slot));
                }
            }
            Expr::FieldAccess { object, field } => {
                self.emit_expr(object)?;
                self.emit_expr(value)?;
                self.emit(&format!(
                    "mv.visitFieldInsn(PUTFIELD, \"js/runtime/JSObject\", \"{}\", \"Ljava/lang/Object;\");",
                    field.name
                ));
            }
            Expr::Index { object, index } => {
                self.emit_expr(object)?;
                self.emit_expr(index)?;
                self.emit_expr(value)?;
                self.emit("mv.visitInsn(AASTORE);");
            }
            _ => {
                self.emit("// complex assignment target");
            }
        }
        Ok(AsmValue::Void)
    }

    fn emit_new(
        &mut self,
        type_expr: &TypeExpr,
        fields: &[(Ident, Spanned<Expr>)],
    ) -> CodeGenResult<Self::Value> {
        let ty = self.torque_to_jvm(type_expr)?;
        if let JvmType::Object(ref class) = ty {
            self.emit(&format!("mv.visitTypeInsn(NEW, \"{}\");", class));
            self.emit("mv.visitInsn(DUP);");
            self.emit(&format!(
                "mv.visitMethodInsn(INVOKESPECIAL, \"{}\", \"<init>\", \"()V\", false);",
                class
            ));

            // Set fields
            for (field_name, field_value) in fields {
                self.emit("mv.visitInsn(DUP);");
                self.emit_expr(field_value)?;
                self.emit(&format!(
                    "mv.visitFieldInsn(PUTFIELD, \"{}\", \"{}\", \"Ljava/lang/Object;\");",
                    class, field_name.name
                ));
            }
        }
        Ok(AsmValue::Stack)
    }

    fn declare_label(&mut self, name: &Ident, params: &[Parameter]) -> CodeGenResult<Self::Label> {
        let java_var = self.fresh_label(&name.name);
        self.emit(&format!("Label {} = new Label();", java_var));

        let label = AsmLabel {
            name: name.name.to_string(),
            java_var,
        };
        self.labels.insert(name.name.to_string(), label.clone());
        Ok(label)
    }

    fn emit_jump(&mut self, label: &Self::Label, args: &[Spanned<Expr>]) -> CodeGenResult<()> {
        for arg in args {
            self.emit_expr(arg)?;
        }
        self.emit(&format!("mv.visitJumpInsn(GOTO, {});", label.java_var));
        Ok(())
    }

    fn begin_label(&mut self, label: &Self::Label) -> CodeGenResult<()> {
        self.emit(&format!("mv.visitLabel({});", label.java_var));
        Ok(())
    }

    fn end_label(&mut self, _label: &Self::Label) -> CodeGenResult<()> {
        // Labels don't need explicit end in JVM
        Ok(())
    }

    fn finalize(&mut self) -> CodeGenResult<Self::Output> {
        // Pop namespace if any
        self.namespace_stack.clear();

        // Convert all class builders to Java source
        let classes: HashMap<String, String> = self.classes
            .iter()
            .map(|(name, builder)| (name.clone(), builder.to_java()))
            .collect();

        Ok(JavaAsmOutput { classes })
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

fn op_to_string(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "add",
        BinaryOp::Sub => "sub",
        BinaryOp::Mul => "mul",
        BinaryOp::Div => "div",
        BinaryOp::Mod => "mod",
        BinaryOp::Eq => "eq",
        BinaryOp::Ne => "ne",
        BinaryOp::Lt => "lt",
        BinaryOp::Le => "le",
        BinaryOp::Gt => "gt",
        BinaryOp::Ge => "ge",
        BinaryOp::And => "and",
        BinaryOp::Or => "or",
        BinaryOp::BitAnd => "bitand",
        BinaryOp::BitOr => "bitor",
        BinaryOp::BitXor => "bitxor",
        BinaryOp::Shl => "shl",
        BinaryOp::Shr => "shr",
        BinaryOp::Ushr => "ushr",
        BinaryOp::Assign => "assign",
    }
}

fn escape_java_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_source;

    #[test]
    fn test_simple_macro_codegen() {
        let source = r#"
            namespace test {
                macro Add(a: int32, b: int32): int32 {
                    return a + b;
                }
            }
        "#;

        let ast = parse_source(source).expect("parse failed");
        let backend = JavaAsmBackend::new("js.builtins");
        let mut codegen = crate::codegen::CodeGenerator::new(backend);

        let output = codegen.generate(&ast).expect("codegen failed");
        assert!(!output.classes.is_empty());

        let test_class = output.classes.get("test").expect("test class not found");
        assert!(test_class.contains("build_Add"));
        assert!(test_class.contains("IADD"));
    }

    #[test]
    fn test_jvm_type_descriptors() {
        assert_eq!(JvmType::Int.descriptor(), "I");
        assert_eq!(JvmType::Long.descriptor(), "J");
        assert_eq!(JvmType::Object("java/lang/String".to_string()).descriptor(), "Ljava/lang/String;");
        assert_eq!(JvmType::Array(Box::new(JvmType::Int)).descriptor(), "[I");
    }
}
