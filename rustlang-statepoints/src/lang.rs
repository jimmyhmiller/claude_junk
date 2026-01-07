//! A tiny language that compiles to LLVM IR with GC statepoints
//!
//! Uses a Rust macro for syntax, compiles to LLVM IR, runs with MMTk GC.

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

    // Variables
    Var(String),
    Let(String, Box<Expr>, Box<Expr>),
    Set(String, Box<Expr>),

    // List operations
    Cons(Box<Expr>, Box<Expr>),
    Car(Box<Expr>),
    Cdr(Box<Expr>),

    // Arithmetic
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Shl(Box<Expr>, Box<Expr>),

    // Comparisons
    Lt(Box<Expr>, Box<Expr>),
    Gt(Box<Expr>, Box<Expr>),
    Le(Box<Expr>, Box<Expr>),
    Eq(Box<Expr>, Box<Expr>),

    // Predicates
    NullP(Box<Expr>),

    // Control flow
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    While(Box<Expr>, Box<Expr>),
    Do(Vec<Expr>),

    // User-defined functions
    Defn(String, Vec<String>, Box<Expr>),
    Call(String, Vec<Expr>),

    // Effects
    Gc,
    PrintList(Box<Expr>),
    Print(Box<Expr>),
    PrintStretchCheck(Box<Expr>, Box<Expr>),
    PrintTreesCheck(Box<Expr>, Box<Expr>, Box<Expr>),
    PrintLongLivedCheck(Box<Expr>, Box<Expr>),
    SetGlobalRoot(Box<Expr>),
    ClearGlobalRoot,

    // Command line args
    GetArg(Box<Expr>),
    Max(Box<Expr>, Box<Expr>),
}

/// Macro for writing expressions in a Lisp-like syntax
#[macro_export]
macro_rules! lang {
    // Literals
    (nil) => { $crate::lang::Expr::Nil };
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

    // Set: (set x <value>)
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

    // Arithmetic
    ((+ $a:tt $b:tt)) => {
        $crate::lang::Expr::Add(Box::new(lang!($a)), Box::new(lang!($b)))
    };
    ((- $a:tt $b:tt)) => {
        $crate::lang::Expr::Sub(Box::new(lang!($a)), Box::new(lang!($b)))
    };
    ((<< $a:tt $b:tt)) => {
        $crate::lang::Expr::Shl(Box::new(lang!($a)), Box::new(lang!($b)))
    };

    // Comparisons
    ((< $a:tt $b:tt)) => {
        $crate::lang::Expr::Lt(Box::new(lang!($a)), Box::new(lang!($b)))
    };
    ((> $a:tt $b:tt)) => {
        $crate::lang::Expr::Gt(Box::new(lang!($a)), Box::new(lang!($b)))
    };
    ((<= $a:tt $b:tt)) => {
        $crate::lang::Expr::Le(Box::new(lang!($a)), Box::new(lang!($b)))
    };
    ((= $a:tt $b:tt)) => {
        $crate::lang::Expr::Eq(Box::new(lang!($a)), Box::new(lang!($b)))
    };

    // Predicates
    ((null? $e:tt)) => {
        $crate::lang::Expr::NullP(Box::new(lang!($e)))
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

    // User-defined functions
    ((defn $name:ident ($($param:ident)*) $body:tt)) => {
        $crate::lang::Expr::Defn(
            stringify!($name).to_string(),
            vec![$(stringify!($param).to_string()),*],
            Box::new(lang!($body))
        )
    };

    // Function calls - varying arities
    ((call $name:ident)) => {
        $crate::lang::Expr::Call(stringify!($name).to_string(), vec![])
    };
    ((call $name:ident $arg1:tt)) => {
        $crate::lang::Expr::Call(stringify!($name).to_string(), vec![lang!($arg1)])
    };
    ((call $name:ident $arg1:tt $arg2:tt)) => {
        $crate::lang::Expr::Call(stringify!($name).to_string(), vec![lang!($arg1), lang!($arg2)])
    };
    ((call $name:ident $arg1:tt $arg2:tt $arg3:tt)) => {
        $crate::lang::Expr::Call(stringify!($name).to_string(), vec![lang!($arg1), lang!($arg2), lang!($arg3)])
    };

    // Effects
    ((gc)) => { $crate::lang::Expr::Gc };
    ((print $e:tt)) => {
        $crate::lang::Expr::Print(Box::new(lang!($e)))
    };
    ((print-list $e:tt)) => {
        $crate::lang::Expr::PrintList(Box::new(lang!($e)))
    };
    ((print-stretch-check $depth:tt $check:tt)) => {
        $crate::lang::Expr::PrintStretchCheck(Box::new(lang!($depth)), Box::new(lang!($check)))
    };
    ((print-trees-check $iters:tt $depth:tt $check:tt)) => {
        $crate::lang::Expr::PrintTreesCheck(Box::new(lang!($iters)), Box::new(lang!($depth)), Box::new(lang!($check)))
    };
    ((print-long-lived-check $depth:tt $check:tt)) => {
        $crate::lang::Expr::PrintLongLivedCheck(Box::new(lang!($depth)), Box::new(lang!($check)))
    };
    ((set-global-root $val:tt)) => {
        $crate::lang::Expr::SetGlobalRoot(Box::new(lang!($val)))
    };
    ((clear-global-root)) => {
        $crate::lang::Expr::ClearGlobalRoot
    };

    // Command line args
    ((get-arg $idx:tt)) => {
        $crate::lang::Expr::GetArg(Box::new(lang!($idx)))
    };
    ((max $a:tt $b:tt)) => {
        $crate::lang::Expr::Max(Box::new(lang!($a)), Box::new(lang!($b)))
    };
}

/// Runtime function references for global mapping
pub struct RuntimeFunctions<'ctx> {
    pub cons_fn: FunctionValue<'ctx>,
    pub cons_safepoint_fn: FunctionValue<'ctx>,
    pub try_alloc_cons_fn: FunctionValue<'ctx>,
    pub car_fn: FunctionValue<'ctx>,
    pub cdr_fn: FunctionValue<'ctx>,
    pub gc_fn: FunctionValue<'ctx>,
    pub gc_with_frame_info_fn: FunctionValue<'ctx>,
    pub print_fn: FunctionValue<'ctx>,
    pub print_list_fn: FunctionValue<'ctx>,
    pub print_stretch_check_fn: FunctionValue<'ctx>,
    pub print_trees_check_fn: FunctionValue<'ctx>,
    pub print_long_lived_check_fn: FunctionValue<'ctx>,
    pub set_global_root_fn: FunctionValue<'ctx>,
    pub clear_global_root_fn: FunctionValue<'ctx>,
    pub get_arg_fn: FunctionValue<'ctx>,
    pub max_fn: FunctionValue<'ctx>,
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
    cons_safepoint_fn: FunctionValue<'ctx>,  // Safepoint-based cons for MMTk
    try_alloc_cons_fn: FunctionValue<'ctx>,  // For simple GC: returns NULL if need GC
    car_fn: FunctionValue<'ctx>,
    cdr_fn: FunctionValue<'ctx>,
    gc_fn: FunctionValue<'ctx>,
    gc_with_roots_fn: FunctionValue<'ctx>,   // For simple GC: takes root slot pointers
    gc_with_frame_info_fn: FunctionValue<'ctx>,  // For simple GC with stack walking
    frameaddress_fn: FunctionValue<'ctx>,
    stacksave_fn: FunctionValue<'ctx>,
    returnaddress_fn: FunctionValue<'ctx>,
    print_fn: FunctionValue<'ctx>,
    print_list_fn: FunctionValue<'ctx>,
    print_stretch_check_fn: FunctionValue<'ctx>,
    print_trees_check_fn: FunctionValue<'ctx>,
    print_long_lived_check_fn: FunctionValue<'ctx>,
    set_global_root_fn: FunctionValue<'ctx>,
    clear_global_root_fn: FunctionValue<'ctx>,
    get_arg_fn: FunctionValue<'ctx>,
    max_fn: FunctionValue<'ctx>,

    // Variable bindings (name -> alloca for ptr addrspace(1))
    variables: HashMap<String, PointerValue<'ctx>>,

    // User-defined functions
    user_functions: HashMap<String, FunctionValue<'ctx>>,

    // Current function being compiled
    current_fn: Option<FunctionValue<'ctx>>,

    // Use simple GC with retry loop (vs MMTk with deferred GC)
    use_simple_gc: bool,
}

impl<'ctx> Compiler<'ctx> {
    pub fn new(context: &'ctx Context, module_name: &str) -> Self {
        Self::new_with_gc_mode(context, module_name, false)
    }

    pub fn new_with_gc_mode(context: &'ctx Context, module_name: &str, use_simple_gc: bool) -> Self {
        let module = context.create_module(module_name);
        let builder = context.create_builder();

        let i64_type = context.i64_type();
        let void_type = context.void_type();
        let gc_ptr_type = context.ptr_type(gc_address_space());

        // Declare runtime functions
        // Use gc_ptr_type for parameters so LLVM tracks them as GC pointers
        let cons_fn = module.add_function(
            "rt_cons_raw_mmtk",
            gc_ptr_type.fn_type(&[gc_ptr_type.into(), gc_ptr_type.into()], false),
            None,
        );

        // For simple GC: try_alloc returns NULL if heap is full
        let try_alloc_cons_fn = module.add_function(
            "rt_try_alloc_cons",
            gc_ptr_type.fn_type(&[gc_ptr_type.into(), gc_ptr_type.into()], false),
            None,
        );

        let car_fn = module.add_function(
            "rt_car",
            gc_ptr_type.fn_type(&[gc_ptr_type.into()], false),
            None,
        );

        let cdr_fn = module.add_function(
            "rt_cdr",
            gc_ptr_type.fn_type(&[gc_ptr_type.into()], false),
            None,
        );

        let gc_fn = module.add_function(
            "rt_gc_mmtk",
            void_type.fn_type(&[], false),
            None,
        );

        // For simple GC: takes pointers to root slots so GC can update them
        let ptr_type = context.ptr_type(inkwell::AddressSpace::default());
        let gc_with_roots_fn = module.add_function(
            "rt_gc_with_roots",
            void_type.fn_type(&[ptr_type.into(), ptr_type.into()], false),
            None,
        );

        // Safepoint-based cons for MMTk: takes slot pointers and frame info
        let cons_safepoint_fn = module.add_function(
            "rt_cons_raw_mmtk_safepoint",
            gc_ptr_type.fn_type(&[
                ptr_type.into(),    // car_slot
                ptr_type.into(),    // cdr_slot
                i64_type.into(),    // fp
                i64_type.into(),    // sp
                i64_type.into(),    // ra
            ], false),
            None,
        );

        // For simple GC with stack walking: takes frame info (fp, sp, ra)
        let gc_with_frame_info_fn = module.add_function(
            "rt_gc_with_frame_info",
            void_type.fn_type(&[i64_type.into(), i64_type.into(), i64_type.into()], false),
            None,
        );

        // LLVM intrinsics for getting frame info
        let frameaddress_fn = module.add_function(
            "llvm.frameaddress.p0",
            ptr_type.fn_type(&[context.i32_type().into()], false),
            None,
        );
        let stacksave_fn = module.add_function(
            "llvm.stacksave.p0",
            ptr_type.fn_type(&[], false),
            None,
        );
        let returnaddress_fn = module.add_function(
            "llvm.returnaddress",
            ptr_type.fn_type(&[context.i32_type().into()], false),
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

        let print_stretch_check_fn = module.add_function(
            "rt_print_stretch_check",
            void_type.fn_type(&[i64_type.into(), i64_type.into()], false),
            None,
        );

        let print_trees_check_fn = module.add_function(
            "rt_print_trees_check",
            void_type.fn_type(&[i64_type.into(), i64_type.into(), i64_type.into()], false),
            None,
        );

        let print_long_lived_check_fn = module.add_function(
            "rt_print_long_lived_check",
            void_type.fn_type(&[i64_type.into(), i64_type.into()], false),
            None,
        );
        let set_global_root_fn = module.add_function(
            "rt_set_global_root",
            void_type.fn_type(&[i64_type.into()], false),
            None,
        );
        let clear_global_root_fn = module.add_function(
            "rt_clear_global_root",
            void_type.fn_type(&[], false),
            None,
        );

        let get_arg_fn = module.add_function(
            "rt_get_arg",
            i64_type.fn_type(&[i64_type.into()], false),
            None,
        );

        let max_fn = module.add_function(
            "rt_max",
            i64_type.fn_type(&[i64_type.into(), i64_type.into()], false),
            None,
        );

        Compiler {
            context,
            module,
            builder,
            gc_ptr_type,
            cons_fn,
            cons_safepoint_fn,
            try_alloc_cons_fn,
            car_fn,
            cdr_fn,
            gc_fn,
            gc_with_roots_fn,
            gc_with_frame_info_fn,
            frameaddress_fn,
            stacksave_fn,
            returnaddress_fn,
            print_fn,
            print_list_fn,
            print_stretch_check_fn,
            print_trees_check_fn,
            print_long_lived_check_fn,
            set_global_root_fn,
            clear_global_root_fn,
            get_arg_fn,
            max_fn,
            variables: HashMap::new(),
            use_simple_gc,
            user_functions: HashMap::new(),
            current_fn: None,
        }
    }

    /// Get runtime functions and module (consumes compiler)
    pub fn finish(self) -> (Module<'ctx>, RuntimeFunctions<'ctx>) {
        let rt = RuntimeFunctions {
            cons_fn: self.cons_fn,
            cons_safepoint_fn: self.cons_safepoint_fn,
            try_alloc_cons_fn: self.try_alloc_cons_fn,
            car_fn: self.car_fn,
            cdr_fn: self.cdr_fn,
            gc_fn: self.gc_fn,
            gc_with_frame_info_fn: self.gc_with_frame_info_fn,
            print_fn: self.print_fn,
            print_list_fn: self.print_list_fn,
            print_stretch_check_fn: self.print_stretch_check_fn,
            print_trees_check_fn: self.print_trees_check_fn,
            print_long_lived_check_fn: self.print_long_lived_check_fn,
            set_global_root_fn: self.set_global_root_fn,
            clear_global_root_fn: self.clear_global_root_fn,
            get_arg_fn: self.get_arg_fn,
            max_fn: self.max_fn,
        };
        (self.module, rt)
    }

    fn spill_live_gc_ptrs(&mut self) -> Vec<(PointerValue<'ctx>, PointerValue<'ctx>)> {
        let mut live = Vec::with_capacity(self.variables.len());
        for (name, slot) in self.variables.iter() {
            let val = self
                .builder
                .build_load(self.gc_ptr_type, *slot, &format!("{}_live", name))
                .unwrap()
                .into_pointer_value();
            live.push((*slot, val));
        }
        live
    }

    fn restore_live_gc_ptrs(&mut self, live: &[(PointerValue<'ctx>, PointerValue<'ctx>)]) {
        for (slot, val) in live {
            self.builder.build_store(*slot, *val).unwrap();
        }
    }

    /// Compile a top-level expression into a function
    pub fn compile_function(&mut self, name: &str, expr: &Expr) -> FunctionValue<'ctx> {
        let i64_type = self.context.i64_type();
        let fn_type = i64_type.fn_type(&[], false);
        let function = self.module.add_function(name, fn_type, None);
        function.set_gc("statepoint-example");
        // Force frame pointer to be preserved for stack walking
        function.add_attribute(
            inkwell::attributes::AttributeLoc::Function,
            self.context.create_string_attribute("frame-pointer", "all"),
        );

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

    fn build_alloca_in_entry(&self, name: &str) -> PointerValue<'ctx> {
        let function = self.current_fn.expect("No current function for alloca");
        let entry = function.get_first_basic_block().expect("Function has no entry block");
        let builder = self.context.create_builder();
        match entry.get_first_instruction() {
            Some(inst) => builder.position_before(&inst),
            None => builder.position_at_end(entry),
        }
        builder.build_alloca(self.gc_ptr_type, name).unwrap()
    }

    /// Convert a GC pointer to i64 (ptrtoint)
    fn gc_ptr_to_i64(&self, ptr: PointerValue<'ctx>) -> IntValue<'ctx> {
        self.builder.build_ptr_to_int(ptr, self.context.i64_type(), "to_i64").unwrap()
    }

    /// Compile an expression, returning its value as a GC pointer
    fn compile_expr(&mut self, expr: &Expr) -> PointerValue<'ctx> {
        let i64_type = self.context.i64_type();

        match expr {
            Expr::Int(n) => {
                let val = i64_type.const_int(make_fixnum(*n), false);
                self.i64_to_gc_ptr(val)
            }
            Expr::Nil => {
                let val = i64_type.const_int(NIL, false);
                self.i64_to_gc_ptr(val)
            }

            Expr::Var(name) => {
                let ptr = self.variables.get(name)
                    .unwrap_or_else(|| panic!("Undefined variable: {}", name));
                self.builder.build_load(self.gc_ptr_type, *ptr, name)
                    .unwrap()
                    .into_pointer_value()
            }

            Expr::Let(name, init, body) => {
                let init_val = self.compile_expr(init);
                let alloca = self.build_alloca_in_entry(name);
                self.builder.build_store(alloca, init_val).unwrap();

                let old = self.variables.insert(name.clone(), alloca);
                let result = self.compile_expr(body);

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
                let car_ptr = self.compile_expr(car);
                let cdr_ptr = self.compile_expr(cdr);
                let live = self.spill_live_gc_ptrs();

                if self.use_simple_gc {
                    // Simple GC: Use retry loop pattern with stack walking
                    // 1. Store car/cdr in stack slots (stackmap will track them)
                    // 2. Try to allocate
                    // 3. If NULL, get frame info and trigger GC at safepoint
                    // 4. GC walks stack using stackmap to find all roots
                    let car_slot = self.build_alloca_in_entry("car_slot");
                    let cdr_slot = self.build_alloca_in_entry("cdr_slot");
                    self.builder.build_store(car_slot, car_ptr).unwrap();
                    self.builder.build_store(cdr_slot, cdr_ptr).unwrap();

                    let current_fn = self.current_fn.unwrap();
                    let retry_bb = self.context.append_basic_block(current_fn, "retry");
                    let need_gc_bb = self.context.append_basic_block(current_fn, "need_gc");
                    let done_bb = self.context.append_basic_block(current_fn, "done");

                    self.builder.build_unconditional_branch(retry_bb).unwrap();

                    // retry:
                    self.builder.position_at_end(retry_bb);
                    let car_val = self.builder
                        .build_load(self.gc_ptr_type, car_slot, "car_val")
                        .unwrap()
                        .into_pointer_value();
                    let cdr_val = self.builder
                        .build_load(self.gc_ptr_type, cdr_slot, "cdr_val")
                        .unwrap()
                        .into_pointer_value();
                    let result = self.builder
                        .build_call(self.try_alloc_cons_fn, &[car_val.into(), cdr_val.into()], "try_cons")
                        .unwrap()
                        .try_as_basic_value()
                        .unwrap_basic()
                        .into_pointer_value();
                    let is_null = self.builder.build_is_null(result, "is_null").unwrap();
                    self.builder.build_conditional_branch(is_null, need_gc_bb, done_bb).unwrap();

                    // need_gc: Get frame info and call GC with stack walking
                    self.builder.position_at_end(need_gc_bb);
                    let i64_type = self.context.i64_type();
                    let i32_type = self.context.i32_type();
                    let zero = i32_type.const_int(0, false);

                    // Get frame pointer
                    let fp_ptr = self.builder
                        .build_call(self.frameaddress_fn, &[zero.into()], "fp_ptr")
                        .unwrap()
                        .try_as_basic_value()
                        .unwrap_basic()
                        .into_pointer_value();
                    let fp = self.builder.build_ptr_to_int(fp_ptr, i64_type, "fp").unwrap();

                    // Get stack pointer
                    let sp_ptr = self.builder
                        .build_call(self.stacksave_fn, &[], "sp_ptr")
                        .unwrap()
                        .try_as_basic_value()
                        .unwrap_basic()
                        .into_pointer_value();
                    let sp = self.builder.build_ptr_to_int(sp_ptr, i64_type, "sp").unwrap();

                    // Get return address
                    let ra_ptr = self.builder
                        .build_call(self.returnaddress_fn, &[zero.into()], "ra_ptr")
                        .unwrap()
                        .try_as_basic_value()
                        .unwrap_basic()
                        .into_pointer_value();
                    let ra = self.builder.build_ptr_to_int(ra_ptr, i64_type, "ra").unwrap();

                    // Call GC with frame info for stack walking
                    self.builder.build_call(
                        self.gc_with_frame_info_fn,
                        &[fp.into(), sp.into(), ra.into()],
                        "gc"
                    ).unwrap();
                    self.builder.build_unconditional_branch(retry_bb).unwrap();

                    // done:
                    self.builder.position_at_end(done_bb);
                    self.restore_live_gc_ptrs(&live);
                    result
                } else {
                    // MMTk: Use safepoint-based allocation with frame info
                    // Store car/cdr in stack slots so GC can find and update them via stackmap
                    let car_slot = self.build_alloca_in_entry("car_slot");
                    let cdr_slot = self.build_alloca_in_entry("cdr_slot");
                    self.builder.build_store(car_slot, car_ptr).unwrap();
                    self.builder.build_store(cdr_slot, cdr_ptr).unwrap();

                    let i64_type = self.context.i64_type();
                    let i32_type = self.context.i32_type();
                    let zero = i32_type.const_int(0, false);

                    // Get frame pointer
                    let fp_ptr = self.builder
                        .build_call(self.frameaddress_fn, &[zero.into()], "fp_ptr")
                        .unwrap()
                        .try_as_basic_value()
                        .unwrap_basic()
                        .into_pointer_value();
                    let fp = self.builder.build_ptr_to_int(fp_ptr, i64_type, "fp").unwrap();

                    // Get stack pointer
                    let sp_ptr = self.builder
                        .build_call(self.stacksave_fn, &[], "sp_ptr")
                        .unwrap()
                        .try_as_basic_value()
                        .unwrap_basic()
                        .into_pointer_value();
                    let sp = self.builder.build_ptr_to_int(sp_ptr, i64_type, "sp").unwrap();

                    // Get return address
                    let ra_ptr = self.builder
                        .build_call(self.returnaddress_fn, &[zero.into()], "ra_ptr")
                        .unwrap()
                        .try_as_basic_value()
                        .unwrap_basic()
                        .into_pointer_value();
                    let ra = self.builder.build_ptr_to_int(ra_ptr, i64_type, "ra").unwrap();

                    // Call safepoint-based cons with slot pointers and frame info
                    // The function reads car/cdr from slots AFTER allocation (in case GC updated them)
                    let result = self.builder
                        .build_call(
                            self.cons_safepoint_fn,
                            &[car_slot.into(), cdr_slot.into(), fp.into(), sp.into(), ra.into()],
                            "cons"
                        )
                        .unwrap()
                        .try_as_basic_value()
                        .unwrap_basic()
                        .into_pointer_value();
                    self.restore_live_gc_ptrs(&live);
                    result
                }
            }

            Expr::Car(e) => {
                let cell_ptr = self.compile_expr(e);
                let live = self.spill_live_gc_ptrs();
                let result = self.builder
                    .build_call(self.car_fn, &[cell_ptr.into()], "car")
                    .unwrap()
                    .try_as_basic_value()
                    .unwrap_basic()
                    .into_pointer_value();
                self.restore_live_gc_ptrs(&live);
                result
            }

            Expr::Cdr(e) => {
                let cell_ptr = self.compile_expr(e);
                let live = self.spill_live_gc_ptrs();
                let result = self.builder
                    .build_call(self.cdr_fn, &[cell_ptr.into()], "cdr")
                    .unwrap()
                    .try_as_basic_value()
                    .unwrap_basic()
                    .into_pointer_value();
                self.restore_live_gc_ptrs(&live);
                result
            }

            Expr::Add(a, b) => {
                let a_ptr = self.compile_expr(a);
                let a_val = self.gc_ptr_to_i64(a_ptr);
                let b_ptr = self.compile_expr(b);
                let b_val = self.gc_ptr_to_i64(b_ptr);
                // For fixnums: a + b - 1 (since tag is 1)
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

            Expr::Shl(a, b) => {
                let a_ptr = self.compile_expr(a);
                let a_val = self.gc_ptr_to_i64(a_ptr);
                let b_ptr = self.compile_expr(b);
                let b_val = self.gc_ptr_to_i64(b_ptr);
                // Tag uses 3 bits (TAG_BITS = 3)
                let tag_bits = i64_type.const_int(3, false);
                let one = i64_type.const_int(1, false);
                // Untag a: a >> 3
                let a_untagged = self.builder.build_right_shift(a_val, tag_bits, false, "a_untag").unwrap();
                // Untag b: b >> 3
                let b_untagged = self.builder.build_right_shift(b_val, tag_bits, false, "b_untag").unwrap();
                // Shift: a << b
                let shifted = self.builder.build_left_shift(a_untagged, b_untagged, "shl").unwrap();
                // Re-tag: (result << 3) | 1
                let result_shifted = self.builder.build_left_shift(shifted, tag_bits, "result_shift").unwrap();
                let result = self.builder.build_or(result_shifted, one, "fixnum_shl").unwrap();
                self.i64_to_gc_ptr(result)
            }

            Expr::Lt(a, b) => {
                let a_ptr = self.compile_expr(a);
                let a_val = self.gc_ptr_to_i64(a_ptr);
                let b_ptr = self.compile_expr(b);
                let b_val = self.gc_ptr_to_i64(b_ptr);
                let cmp = self.builder.build_int_compare(IntPredicate::SLT, a_val, b_val, "lt").unwrap();
                let true_val = self.i64_to_gc_ptr(i64_type.const_int(TRUE, false));
                let false_val = self.i64_to_gc_ptr(i64_type.const_int(FALSE, false));
                self.builder.build_select(cmp, true_val, false_val, "lt_result")
                    .unwrap()
                    .into_pointer_value()
            }

            Expr::Gt(a, b) => {
                let a_ptr = self.compile_expr(a);
                let a_val = self.gc_ptr_to_i64(a_ptr);
                let b_ptr = self.compile_expr(b);
                let b_val = self.gc_ptr_to_i64(b_ptr);
                let cmp = self.builder.build_int_compare(IntPredicate::SGT, a_val, b_val, "gt").unwrap();
                let true_val = self.i64_to_gc_ptr(i64_type.const_int(TRUE, false));
                let false_val = self.i64_to_gc_ptr(i64_type.const_int(FALSE, false));
                self.builder.build_select(cmp, true_val, false_val, "gt_result")
                    .unwrap()
                    .into_pointer_value()
            }

            Expr::Le(a, b) => {
                let a_ptr = self.compile_expr(a);
                let a_val = self.gc_ptr_to_i64(a_ptr);
                let b_ptr = self.compile_expr(b);
                let b_val = self.gc_ptr_to_i64(b_ptr);
                let cmp = self.builder.build_int_compare(IntPredicate::SLE, a_val, b_val, "le").unwrap();
                let true_val = self.i64_to_gc_ptr(i64_type.const_int(TRUE, false));
                let false_val = self.i64_to_gc_ptr(i64_type.const_int(FALSE, false));
                self.builder.build_select(cmp, true_val, false_val, "le_result")
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

            Expr::NullP(e) => {
                let val_ptr = self.compile_expr(e);
                let val = self.gc_ptr_to_i64(val_ptr);
                let nil_val = i64_type.const_int(NIL, false);
                let cmp = self.builder.build_int_compare(IntPredicate::EQ, val, nil_val, "nullp").unwrap();
                let true_val = self.i64_to_gc_ptr(i64_type.const_int(TRUE, false));
                let false_val = self.i64_to_gc_ptr(i64_type.const_int(FALSE, false));
                self.builder.build_select(cmp, true_val, false_val, "nullp_result")
                    .unwrap()
                    .into_pointer_value()
            }

            Expr::If(cond, then_expr, else_expr) => {
                let function = self.current_fn.unwrap();
                let then_bb = self.context.append_basic_block(function, "then");
                let else_bb = self.context.append_basic_block(function, "else");
                let merge_bb = self.context.append_basic_block(function, "ifcont");

                let cond_ptr = self.compile_expr(cond);
                let cond_val = self.gc_ptr_to_i64(cond_ptr);
                let false_val = i64_type.const_int(FALSE, false);
                let cmp = self.builder.build_int_compare(IntPredicate::NE, cond_val, false_val, "ifcond").unwrap();
                self.builder.build_conditional_branch(cmp, then_bb, else_bb).unwrap();

                // Then branch
                self.builder.position_at_end(then_bb);
                let then_val = self.compile_expr(then_expr);
                self.builder.build_unconditional_branch(merge_bb).unwrap();
                let then_bb_end = self.builder.get_insert_block().unwrap();

                // Else branch
                self.builder.position_at_end(else_bb);
                let else_val = self.compile_expr(else_expr);
                self.builder.build_unconditional_branch(merge_bb).unwrap();
                let else_bb_end = self.builder.get_insert_block().unwrap();

                // Merge
                self.builder.position_at_end(merge_bb);
                let phi = self.builder.build_phi(self.gc_ptr_type, "ifresult").unwrap();
                phi.add_incoming(&[(&then_val, then_bb_end), (&else_val, else_bb_end)]);
                phi.as_basic_value().into_pointer_value()
            }

            Expr::While(cond, body) => {
                let function = self.current_fn.unwrap();
                let cond_bb = self.context.append_basic_block(function, "whilecond");
                let body_bb = self.context.append_basic_block(function, "whilebody");
                let end_bb = self.context.append_basic_block(function, "whileend");

                self.builder.build_unconditional_branch(cond_bb).unwrap();

                self.builder.position_at_end(cond_bb);
                let cond_ptr = self.compile_expr(cond);
                let cond_val = self.gc_ptr_to_i64(cond_ptr);
                let false_val = i64_type.const_int(FALSE, false);
                let cmp = self.builder.build_int_compare(IntPredicate::NE, cond_val, false_val, "whilecond").unwrap();
                self.builder.build_conditional_branch(cmp, body_bb, end_bb).unwrap();

                self.builder.position_at_end(body_bb);
                self.compile_expr(body);
                self.builder.build_unconditional_branch(cond_bb).unwrap();

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

            Expr::Defn(name, params, body) => {
                // Save current state
                let saved_fn = self.current_fn;
                let saved_vars = std::mem::take(&mut self.variables);

                // Create the user function
                let param_types: Vec<_> = params.iter().map(|_| i64_type.into()).collect();
                let fn_type = i64_type.fn_type(&param_types, false);
                let function = self.module.add_function(name, fn_type, None);
                function.set_gc("statepoint-example");
                // Force frame pointer to be preserved for stack walking
                function.add_attribute(
                    inkwell::attributes::AttributeLoc::Function,
                    self.context.create_string_attribute("frame-pointer", "all"),
                );

                // Store in user_functions map BEFORE compiling body (for recursion)
                self.user_functions.insert(name.clone(), function);

                let entry = self.context.append_basic_block(function, "entry");
                self.builder.position_at_end(entry);
                self.current_fn = Some(function);

                // Bind parameters to allocas
                for (i, param_name) in params.iter().enumerate() {
                    let param_val = function.get_nth_param(i as u32).unwrap().into_int_value();
                    let param_ptr = self.i64_to_gc_ptr(param_val);
                    let alloca = self.builder.build_alloca(self.gc_ptr_type, param_name).unwrap();
                    self.builder.build_store(alloca, param_ptr).unwrap();
                    self.variables.insert(param_name.clone(), alloca);
                }

                // Compile body
                let result_ptr = self.compile_expr(body);
                let result = self.gc_ptr_to_i64(result_ptr);
                self.builder.build_return(Some(&result)).unwrap();

                // Restore state
                self.current_fn = saved_fn;
                self.variables = saved_vars;

                // Position builder back at the correct spot if we were in a function
                if let Some(fn_val) = saved_fn {
                    if let Some(last_bb) = fn_val.get_last_basic_block() {
                        self.builder.position_at_end(last_bb);
                    }
                }

                // Defn returns nil
                self.i64_to_gc_ptr(i64_type.const_int(NIL, false))
            }

            Expr::Call(name, args) => {
                let function = *self.user_functions.get(name)
                    .unwrap_or_else(|| panic!("Undefined function: {}", name));

                // Compile arguments
                let mut arg_vals: Vec<inkwell::values::BasicMetadataValueEnum> = Vec::new();
                for a in args {
                    let ptr = self.compile_expr(a);
                    arg_vals.push(self.gc_ptr_to_i64(ptr).into());
                }

                let live = self.spill_live_gc_ptrs();
                let result = self.builder
                    .build_call(function, &arg_vals, "call")
                    .unwrap()
                    .try_as_basic_value()
                    .unwrap_basic()
                    .into_int_value();
                self.restore_live_gc_ptrs(&live);

                self.i64_to_gc_ptr(result)
            }

            Expr::Gc => {
                // Load all GC pointers into registers before the GC call
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

                self.builder.build_call(self.gc_fn, &[], "gc").unwrap();

                // Store relocated values back
                for (name, val) in &live_values {
                    let slot = *self.variables.get(name).unwrap();
                    self.builder.build_store(slot, *val).unwrap();
                }

                self.i64_to_gc_ptr(i64_type.const_int(NIL, false))
            }

            Expr::Print(e) => {
                let val = self.compile_expr(e);
                let val_i64 = self.gc_ptr_to_i64(val);
                let live = self.spill_live_gc_ptrs();
                self.builder.build_call(self.print_fn, &[val_i64.into()], "").unwrap();
                self.restore_live_gc_ptrs(&live);
                val
            }

            Expr::PrintList(e) => {
                let val = self.compile_expr(e);
                let val_i64 = self.gc_ptr_to_i64(val);
                let live = self.spill_live_gc_ptrs();
                self.builder.build_call(self.print_list_fn, &[val_i64.into()], "").unwrap();
                self.restore_live_gc_ptrs(&live);
                val
            }

            Expr::PrintStretchCheck(depth, check) => {
                let depth_ptr = self.compile_expr(depth);
                let depth_val = self.gc_ptr_to_i64(depth_ptr);
                let check_ptr = self.compile_expr(check);
                let check_val = self.gc_ptr_to_i64(check_ptr);
                let live = self.spill_live_gc_ptrs();
                self.builder.build_call(
                    self.print_stretch_check_fn,
                    &[depth_val.into(), check_val.into()],
                    ""
                ).unwrap();
                self.restore_live_gc_ptrs(&live);
                self.i64_to_gc_ptr(i64_type.const_int(NIL, false))
            }

            Expr::PrintTreesCheck(iters, depth, check) => {
                let iters_ptr = self.compile_expr(iters);
                let iters_val = self.gc_ptr_to_i64(iters_ptr);
                let depth_ptr = self.compile_expr(depth);
                let depth_val = self.gc_ptr_to_i64(depth_ptr);
                let check_ptr = self.compile_expr(check);
                let check_val = self.gc_ptr_to_i64(check_ptr);
                let live = self.spill_live_gc_ptrs();
                self.builder.build_call(
                    self.print_trees_check_fn,
                    &[iters_val.into(), depth_val.into(), check_val.into()],
                    ""
                ).unwrap();
                self.restore_live_gc_ptrs(&live);
                self.i64_to_gc_ptr(i64_type.const_int(NIL, false))
            }

            Expr::PrintLongLivedCheck(depth, check) => {
                let depth_ptr = self.compile_expr(depth);
                let depth_val = self.gc_ptr_to_i64(depth_ptr);
                let check_ptr = self.compile_expr(check);
                let check_val = self.gc_ptr_to_i64(check_ptr);
                let live = self.spill_live_gc_ptrs();
                self.builder.build_call(
                    self.print_long_lived_check_fn,
                    &[depth_val.into(), check_val.into()],
                    ""
                ).unwrap();
                self.restore_live_gc_ptrs(&live);
                self.i64_to_gc_ptr(i64_type.const_int(NIL, false))
            }

            Expr::SetGlobalRoot(val) => {
                let ptr = self.compile_expr(val);
                let tagged = self.gc_ptr_to_i64(ptr);
                let live = self.spill_live_gc_ptrs();
                self.builder
                    .build_call(self.set_global_root_fn, &[tagged.into()], "set_global_root")
                    .unwrap();
                self.restore_live_gc_ptrs(&live);
                self.i64_to_gc_ptr(i64_type.const_int(NIL, false))
            }

            Expr::ClearGlobalRoot => {
                let live = self.spill_live_gc_ptrs();
                self.builder
                    .build_call(self.clear_global_root_fn, &[], "clear_global_root")
                    .unwrap();
                self.restore_live_gc_ptrs(&live);
                self.i64_to_gc_ptr(i64_type.const_int(NIL, false))
            }

            Expr::GetArg(idx) => {
                let idx_ptr = self.compile_expr(idx);
                let idx_val = self.gc_ptr_to_i64(idx_ptr);
                let live = self.spill_live_gc_ptrs();
                let result = self.builder
                    .build_call(self.get_arg_fn, &[idx_val.into()], "get_arg")
                    .unwrap()
                    .try_as_basic_value()
                    .unwrap_basic()
                    .into_int_value();
                self.restore_live_gc_ptrs(&live);
                self.i64_to_gc_ptr(result)
            }

            Expr::Max(a, b) => {
                let a_ptr = self.compile_expr(a);
                let a_val = self.gc_ptr_to_i64(a_ptr);
                let b_ptr = self.compile_expr(b);
                let b_val = self.gc_ptr_to_i64(b_ptr);
                let live = self.spill_live_gc_ptrs();
                let result = self.builder
                    .build_call(self.max_fn, &[a_val.into(), b_val.into()], "max")
                    .unwrap()
                    .try_as_basic_value()
                    .unwrap_basic()
                    .into_int_value();
                self.restore_live_gc_ptrs(&live);
                self.i64_to_gc_ptr(result)
            }
        }
    }
}
