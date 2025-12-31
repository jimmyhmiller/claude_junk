//! A tiny language that compiles to LLVM IR with GC statepoints
//!
//! Uses a Rust macro for syntax, compiles to LLVM IR, runs with MMTk GC.
//!
//! Key insight: To make LLVM's statepoint pass track our roots, we need to use
//! `ptr addrspace(1)` types for values that might contain GC pointers.
//! Our tagged values can hold either fixnums or pointers, so we treat all
//! variables as potentially holding GC pointers.

use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::values::{FunctionValue, IntValue, PointerValue};
use inkwell::types::PointerType;
use inkwell::AddressSpace;
use inkwell::IntPredicate;
use std::collections::HashMap;

use crate::tagged_value::{make_fixnum, NIL, TRUE, FALSE};

/// Address space for GC-tracked pointers
fn gc_address_space() -> AddressSpace {
    AddressSpace::from(1)
}

/// AST for our tiny language
#[derive(Debug, Clone)]
pub enum Expr {
    // Literals
    Int(i64),
    Nil,
    True,
    False,

    // Variables
    Var(String),
    Let(String, Box<Expr>, Box<Expr>),
    Set(String, Box<Expr>),

    // List operations
    Cons(Box<Expr>, Box<Expr>),
    Car(Box<Expr>),
    Cdr(Box<Expr>),
    IsNil(Box<Expr>),

    // Arithmetic
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Lt(Box<Expr>, Box<Expr>),
    Eq(Box<Expr>, Box<Expr>),

    // Control flow
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    While(Box<Expr>, Box<Expr>),
    Do(Vec<Expr>),

    // Effects
    Gc,
    Print(Box<Expr>),
    PrintList(Box<Expr>),
}

/// Macro for writing expressions in a Lisp-like syntax
#[macro_export]
macro_rules! lang {
    // Literals
    (nil) => { $crate::lang::Expr::Nil };
    (true) => { $crate::lang::Expr::True };
    (false) => { $crate::lang::Expr::False };
    ($int:literal) => { $crate::lang::Expr::Int($int) };

    // Variables (identifiers that aren't keywords)
    ($var:ident) => { $crate::lang::Expr::Var(stringify!($var).to_string()) };

    // Let binding: (let x <init> <body>)
    ((let $name:ident $init:tt $body:tt)) => {
        $crate::lang::Expr::Let(
            stringify!($name).to_string(),
            Box::new(lang!($init)),
            Box::new(lang!($body))
        )
    };

    // Set: (set! x <value>)
    ((set $name:ident $value:tt)) => {
        $crate::lang::Expr::Set(
            stringify!($name).to_string(),
            Box::new(lang!($value))
        )
    };

    // List operations
    ((cons $car:tt $cdr:tt)) => {
        $crate::lang::Expr::Cons(Box::new(lang!($car)), Box::new(lang!($cdr)))
    };
    ((car $e:tt)) => {
        $crate::lang::Expr::Car(Box::new(lang!($e)))
    };
    ((cdr $e:tt)) => {
        $crate::lang::Expr::Cdr(Box::new(lang!($e)))
    };
    ((nil? $e:tt)) => {
        $crate::lang::Expr::IsNil(Box::new(lang!($e)))
    };

    // Arithmetic
    ((+ $a:tt $b:tt)) => {
        $crate::lang::Expr::Add(Box::new(lang!($a)), Box::new(lang!($b)))
    };
    ((- $a:tt $b:tt)) => {
        $crate::lang::Expr::Sub(Box::new(lang!($a)), Box::new(lang!($b)))
    };
    ((* $a:tt $b:tt)) => {
        $crate::lang::Expr::Mul(Box::new(lang!($a)), Box::new(lang!($b)))
    };
    ((< $a:tt $b:tt)) => {
        $crate::lang::Expr::Lt(Box::new(lang!($a)), Box::new(lang!($b)))
    };
    ((= $a:tt $b:tt)) => {
        $crate::lang::Expr::Eq(Box::new(lang!($a)), Box::new(lang!($b)))
    };

    // Control flow
    ((if $cond:tt $then:tt $else:tt)) => {
        $crate::lang::Expr::If(
            Box::new(lang!($cond)),
            Box::new(lang!($then)),
            Box::new(lang!($else))
        )
    };
    ((while $cond:tt $body:tt)) => {
        $crate::lang::Expr::While(
            Box::new(lang!($cond)),
            Box::new(lang!($body))
        )
    };
    ((do $($expr:tt)*)) => {
        $crate::lang::Expr::Do(vec![$(lang!($expr)),*])
    };

    // Effects
    ((gc)) => { $crate::lang::Expr::Gc };
    ((print $e:tt)) => {
        $crate::lang::Expr::Print(Box::new(lang!($e)))
    };
    ((print-list $e:tt)) => {
        $crate::lang::Expr::PrintList(Box::new(lang!($e)))
    };
}

/// Runtime function references for global mapping
pub struct RuntimeFunctions<'ctx> {
    pub cons_fn: FunctionValue<'ctx>,
    pub car_fn: FunctionValue<'ctx>,
    pub cdr_fn: FunctionValue<'ctx>,
    pub gc_fn: FunctionValue<'ctx>,
    pub print_fn: FunctionValue<'ctx>,
    pub print_list_fn: FunctionValue<'ctx>,
}

/// Compiler from Expr to LLVM IR
pub struct Compiler<'ctx> {
    context: &'ctx Context,
    module: Module<'ctx>,
    builder: Builder<'ctx>,

    // Types
    gc_ptr_type: PointerType<'ctx>,

    // Runtime functions
    cons_fn: FunctionValue<'ctx>,
    car_fn: FunctionValue<'ctx>,
    cdr_fn: FunctionValue<'ctx>,
    gc_fn: FunctionValue<'ctx>,
    print_fn: FunctionValue<'ctx>,
    print_list_fn: FunctionValue<'ctx>,

    // Variable bindings (name -> alloca for ptr addrspace(1))
    variables: HashMap<String, PointerValue<'ctx>>,

    // Current function being compiled
    current_fn: Option<FunctionValue<'ctx>>,
}

impl<'ctx> Compiler<'ctx> {
    pub fn new(context: &'ctx Context, module_name: &str) -> Self {
        let module = context.create_module(module_name);
        let builder = context.create_builder();

        let i64_type = context.i64_type();
        let void_type = context.void_type();
        let gc_ptr_type = context.ptr_type(gc_address_space());

        // Declare runtime functions
        // cons returns a GC pointer (ptr addrspace(1)) - this is key for statepoint tracking!
        let cons_fn = module.add_function(
            "rt_cons_raw_mmtk",
            gc_ptr_type.fn_type(&[i64_type.into(), i64_type.into()], false),
            None,
        );

        let car_fn = module.add_function(
            "rt_car",
            i64_type.fn_type(&[i64_type.into()], false),
            None,
        );

        let cdr_fn = module.add_function(
            "rt_cdr",
            i64_type.fn_type(&[i64_type.into()], false),
            None,
        );

        let gc_fn = module.add_function(
            "rt_gc_mmtk",
            void_type.fn_type(&[], false),
            None,
        );

        let print_fn = module.add_function(
            "rt_print",
            void_type.fn_type(&[i64_type.into()], false),
            None,
        );

        let print_list_fn = module.add_function(
            "rt_print_list",
            void_type.fn_type(&[i64_type.into()], false),
            None,
        );

        Compiler {
            context,
            module,
            builder,
            gc_ptr_type,
            cons_fn,
            car_fn,
            cdr_fn,
            gc_fn,
            print_fn,
            print_list_fn,
            variables: HashMap::new(),
            current_fn: None,
        }
    }

    pub fn module(&self) -> &Module<'ctx> {
        &self.module
    }

    pub fn into_module(self) -> Module<'ctx> {
        self.module
    }

    /// Get runtime functions and module (consumes compiler)
    pub fn finish(self) -> (Module<'ctx>, RuntimeFunctions<'ctx>) {
        let rt = RuntimeFunctions {
            cons_fn: self.cons_fn,
            car_fn: self.car_fn,
            cdr_fn: self.cdr_fn,
            gc_fn: self.gc_fn,
            print_fn: self.print_fn,
            print_list_fn: self.print_list_fn,
        };
        (self.module, rt)
    }

    /// Compile a top-level expression into a function
    pub fn compile_function(&mut self, name: &str, expr: &Expr) -> FunctionValue<'ctx> {
        let i64_type = self.context.i64_type();
        let fn_type = i64_type.fn_type(&[], false);
        let function = self.module.add_function(name, fn_type, None);
        function.set_gc("statepoint-example");

        let entry = self.context.append_basic_block(function, "entry");
        self.builder.position_at_end(entry);

        self.current_fn = Some(function);
        self.variables.clear();

        // Compile expression - result is a GC pointer
        let result_ptr = self.compile_expr(expr);
        // Convert to i64 for return
        let result = self.builder.build_ptr_to_int(result_ptr, i64_type, "result_i64").unwrap();
        self.builder.build_return(Some(&result)).unwrap();

        function
    }

    /// Convert an i64 value to a GC pointer (inttoptr)
    fn i64_to_gc_ptr(&self, val: IntValue<'ctx>) -> PointerValue<'ctx> {
        self.builder.build_int_to_ptr(val, self.gc_ptr_type, "to_gcptr").unwrap()
    }

    /// Convert a GC pointer to i64 (ptrtoint)
    fn gc_ptr_to_i64(&self, ptr: PointerValue<'ctx>) -> IntValue<'ctx> {
        self.builder.build_ptr_to_int(ptr, self.context.i64_type(), "to_i64").unwrap()
    }

    /// Compile an expression, returning its value as a GC pointer
    /// All tagged values are represented as ptr addrspace(1) for statepoint tracking
    fn compile_expr(&mut self, expr: &Expr) -> PointerValue<'ctx> {
        let i64_type = self.context.i64_type();

        match expr {
            // Literals - convert to GC pointer representation
            Expr::Int(n) => {
                let val = i64_type.const_int(make_fixnum(*n), false);
                self.i64_to_gc_ptr(val)
            }
            Expr::Nil => {
                let val = i64_type.const_int(NIL, false);
                self.i64_to_gc_ptr(val)
            }
            Expr::True => {
                let val = i64_type.const_int(TRUE, false);
                self.i64_to_gc_ptr(val)
            }
            Expr::False => {
                let val = i64_type.const_int(FALSE, false);
                self.i64_to_gc_ptr(val)
            }

            Expr::Var(name) => {
                let ptr = self.variables.get(name)
                    .unwrap_or_else(|| panic!("Undefined variable: {}", name));
                // Load as GC pointer
                self.builder.build_load(self.gc_ptr_type, *ptr, name)
                    .unwrap()
                    .into_pointer_value()
            }

            Expr::Let(name, init, body) => {
                // Compile initializer - result is a GC pointer
                let init_val = self.compile_expr(init);

                // Create alloca for variable - stores ptr addrspace(1)
                let alloca = self.builder.build_alloca(self.gc_ptr_type, name).unwrap();
                self.builder.build_store(alloca, init_val).unwrap();

                // Add to scope
                let old = self.variables.insert(name.clone(), alloca);

                // Compile body
                let result = self.compile_expr(body);

                // Restore old binding (if any)
                if let Some(old_val) = old {
                    self.variables.insert(name.clone(), old_val);
                } else {
                    self.variables.remove(name);
                }

                result
            }

            Expr::Set(name, value) => {
                let val = self.compile_expr(value);
                let ptr = self.variables.get(name)
                    .unwrap_or_else(|| panic!("Undefined variable: {}", name));
                self.builder.build_store(*ptr, val).unwrap();
                val
            }

            Expr::Cons(car, cdr) => {
                // Convert GC pointers to i64 for function call
                let car_ptr = self.compile_expr(car);
                let car_val = self.gc_ptr_to_i64(car_ptr);
                let cdr_ptr = self.compile_expr(cdr);
                let cdr_val = self.gc_ptr_to_i64(cdr_ptr);
                // cons returns a GC pointer directly
                self.builder
                    .build_call(self.cons_fn, &[car_val.into(), cdr_val.into()], "cons")
                    .unwrap()
                    .try_as_basic_value()
                    .left()
                    .unwrap()
                    .into_pointer_value()
            }

            Expr::Car(e) => {
                let ptr = self.compile_expr(e);
                let val = self.gc_ptr_to_i64(ptr);
                let result = self.builder
                    .build_call(self.car_fn, &[val.into()], "car")
                    .unwrap()
                    .try_as_basic_value()
                    .left()
                    .unwrap()
                    .into_int_value();
                self.i64_to_gc_ptr(result)
            }

            Expr::Cdr(e) => {
                let ptr = self.compile_expr(e);
                let val = self.gc_ptr_to_i64(ptr);
                let result = self.builder
                    .build_call(self.cdr_fn, &[val.into()], "cdr")
                    .unwrap()
                    .try_as_basic_value()
                    .left()
                    .unwrap()
                    .into_int_value();
                self.i64_to_gc_ptr(result)
            }

            Expr::IsNil(e) => {
                let ptr = self.compile_expr(e);
                let val = self.gc_ptr_to_i64(ptr);
                let nil = i64_type.const_int(NIL, false);
                let cmp = self.builder.build_int_compare(IntPredicate::EQ, val, nil, "isnil").unwrap();
                // Convert i1 to tagged bool as GC pointer
                let true_val = self.i64_to_gc_ptr(i64_type.const_int(TRUE, false));
                let false_val = self.i64_to_gc_ptr(i64_type.const_int(FALSE, false));
                self.builder.build_select(cmp, true_val, false_val, "isnil_result")
                    .unwrap()
                    .into_pointer_value()
            }

            Expr::Add(a, b) => {
                let a_ptr = self.compile_expr(a);
                let a_val = self.gc_ptr_to_i64(a_ptr);
                let b_ptr = self.compile_expr(b);
                let b_val = self.gc_ptr_to_i64(b_ptr);
                // For fixnums: a + b - 1 (since both have low bit set)
                let sum = self.builder.build_int_add(a_val, b_val, "sum").unwrap();
                let one = i64_type.const_int(1, false);
                let result = self.builder.build_int_sub(sum, one, "fixnum_add").unwrap();
                self.i64_to_gc_ptr(result)
            }

            Expr::Sub(a, b) => {
                let a_ptr = self.compile_expr(a);
                let a_val = self.gc_ptr_to_i64(a_ptr);
                let b_ptr = self.compile_expr(b);
                let b_val = self.gc_ptr_to_i64(b_ptr);
                // For fixnums: a - b + 1
                let diff = self.builder.build_int_sub(a_val, b_val, "diff").unwrap();
                let one = i64_type.const_int(1, false);
                let result = self.builder.build_int_add(diff, one, "fixnum_sub").unwrap();
                self.i64_to_gc_ptr(result)
            }

            Expr::Mul(a, b) => {
                let a_ptr = self.compile_expr(a);
                let a_val = self.gc_ptr_to_i64(a_ptr);
                let b_ptr = self.compile_expr(b);
                let b_val = self.gc_ptr_to_i64(b_ptr);
                // For fixnums: ((a >> 1) * (b >> 1)) << 1 | 1
                let one = i64_type.const_int(1, false);
                let a_untagged = self.builder.build_right_shift(a_val, one, false, "a_untag").unwrap();
                let b_untagged = self.builder.build_right_shift(b_val, one, false, "b_untag").unwrap();
                let prod = self.builder.build_int_mul(a_untagged, b_untagged, "prod").unwrap();
                let shifted = self.builder.build_left_shift(prod, one, "shifted").unwrap();
                let result = self.builder.build_or(shifted, one, "fixnum_mul").unwrap();
                self.i64_to_gc_ptr(result)
            }

            Expr::Lt(a, b) => {
                let a_ptr = self.compile_expr(a);
                let a_val = self.gc_ptr_to_i64(a_ptr);
                let b_ptr = self.compile_expr(b);
                let b_val = self.gc_ptr_to_i64(b_ptr);
                // Fixnums can be compared directly (tag doesn't affect ordering)
                let cmp = self.builder.build_int_compare(IntPredicate::SLT, a_val, b_val, "lt").unwrap();
                let true_val = self.i64_to_gc_ptr(i64_type.const_int(TRUE, false));
                let false_val = self.i64_to_gc_ptr(i64_type.const_int(FALSE, false));
                self.builder.build_select(cmp, true_val, false_val, "lt_result")
                    .unwrap()
                    .into_pointer_value()
            }

            Expr::Eq(a, b) => {
                let a_ptr = self.compile_expr(a);
                let a_val = self.gc_ptr_to_i64(a_ptr);
                let b_ptr = self.compile_expr(b);
                let b_val = self.gc_ptr_to_i64(b_ptr);
                let cmp = self.builder.build_int_compare(IntPredicate::EQ, a_val, b_val, "eq").unwrap();
                let true_val = self.i64_to_gc_ptr(i64_type.const_int(TRUE, false));
                let false_val = self.i64_to_gc_ptr(i64_type.const_int(FALSE, false));
                self.builder.build_select(cmp, true_val, false_val, "eq_result")
                    .unwrap()
                    .into_pointer_value()
            }

            Expr::If(cond, then_expr, else_expr) => {
                let cond_ptr = self.compile_expr(cond);
                let cond_val = self.gc_ptr_to_i64(cond_ptr);
                let false_val = i64_type.const_int(FALSE, false);
                let cmp = self.builder.build_int_compare(IntPredicate::NE, cond_val, false_val, "ifcond").unwrap();

                let function = self.current_fn.unwrap();
                let then_bb = self.context.append_basic_block(function, "then");
                let else_bb = self.context.append_basic_block(function, "else");
                let merge_bb = self.context.append_basic_block(function, "ifcont");

                self.builder.build_conditional_branch(cmp, then_bb, else_bb).unwrap();

                // Then branch
                self.builder.position_at_end(then_bb);
                let then_val = self.compile_expr(then_expr);
                self.builder.build_unconditional_branch(merge_bb).unwrap();
                let then_bb = self.builder.get_insert_block().unwrap();

                // Else branch
                self.builder.position_at_end(else_bb);
                let else_val = self.compile_expr(else_expr);
                self.builder.build_unconditional_branch(merge_bb).unwrap();
                let else_bb = self.builder.get_insert_block().unwrap();

                // Merge - phi of GC pointers
                self.builder.position_at_end(merge_bb);
                let phi = self.builder.build_phi(self.gc_ptr_type, "iftmp").unwrap();
                phi.add_incoming(&[(&then_val, then_bb), (&else_val, else_bb)]);
                phi.as_basic_value().into_pointer_value()
            }

            Expr::While(cond, body) => {
                let function = self.current_fn.unwrap();
                let cond_bb = self.context.append_basic_block(function, "whilecond");
                let body_bb = self.context.append_basic_block(function, "whilebody");
                let end_bb = self.context.append_basic_block(function, "whileend");

                self.builder.build_unconditional_branch(cond_bb).unwrap();

                // Condition
                self.builder.position_at_end(cond_bb);
                let cond_ptr = self.compile_expr(cond);
                let cond_val = self.gc_ptr_to_i64(cond_ptr);
                let false_val = i64_type.const_int(FALSE, false);
                let cmp = self.builder.build_int_compare(IntPredicate::NE, cond_val, false_val, "whilecond").unwrap();
                self.builder.build_conditional_branch(cmp, body_bb, end_bb).unwrap();

                // Body
                self.builder.position_at_end(body_bb);
                self.compile_expr(body);
                self.builder.build_unconditional_branch(cond_bb).unwrap();

                // End - return nil as GC pointer
                self.builder.position_at_end(end_bb);
                self.i64_to_gc_ptr(i64_type.const_int(NIL, false))
            }

            Expr::Do(exprs) => {
                let mut result = self.i64_to_gc_ptr(i64_type.const_int(NIL, false));
                for e in exprs {
                    result = self.compile_expr(e);
                }
                result
            }

            Expr::Gc => {
                // To make statepoints work, we need all GC pointers to be "live"
                // (in registers) across the GC call. Load all variables into registers
                // before the call, and store them back after.

                // Collect all current variables and their values
                let var_names: Vec<String> = self.variables.keys().cloned().collect();
                let mut live_values: Vec<(String, PointerValue<'ctx>)> = Vec::new();

                for name in &var_names {
                    let slot = *self.variables.get(name).unwrap();
                    let val = self.builder
                        .build_load(self.gc_ptr_type, slot, &format!("{}_live", name))
                        .unwrap()
                        .into_pointer_value();
                    live_values.push((name.clone(), val));
                }

                // Call GC - the live values are in registers now
                self.builder.build_call(self.gc_fn, &[], "gc").unwrap();

                // After GC, the live values may have been relocated.
                // Store them back to their slots.
                // Note: Statepoint pass will transform the call and insert gc.relocate
                // intrinsics that provide the new values.
                for (name, val) in &live_values {
                    let slot = *self.variables.get(name).unwrap();
                    // In the transformed code, 'val' will be replaced with the relocated value
                    self.builder.build_store(slot, *val).unwrap();
                }

                self.i64_to_gc_ptr(i64_type.const_int(NIL, false))
            }

            Expr::Print(e) => {
                let val = self.compile_expr(e);
                let val_i64 = self.gc_ptr_to_i64(val);
                self.builder.build_call(self.print_fn, &[val_i64.into()], "").unwrap();
                val
            }

            Expr::PrintList(e) => {
                let val = self.compile_expr(e);
                let val_i64 = self.gc_ptr_to_i64(val);
                self.builder.build_call(self.print_list_fn, &[val_i64.into()], "").unwrap();
                val
            }
        }
    }
}
