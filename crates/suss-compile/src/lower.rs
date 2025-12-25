//! Lowering from analyzed Suss to IR
//!
//! Converts analyzed s-expressions into the IR representation.

use std::collections::HashMap;

use num_traits::ToPrimitive;
use suss_core::{Edn, Number};

use crate::analyze::{AnalyzedModule, AnalyzedFunction, AnalyzedGlobal};
use crate::error::{CompileError, CompileResult};
use crate::ir::{Module, Function, Global, Import, Expr, Type, BinOp, UnOp};

/// Lower analyzed module to IR
pub fn lower(module: &AnalyzedModule) -> CompileResult<Module> {
    let mut lowerer = Lowerer::new();
    lowerer.lower_module(module)
}

struct Lowerer {
    module: Module,
    /// Map from symbol name to (local index, type) for current function
    locals: HashMap<String, (u32, Type)>,
    /// Type info for anonymous locals (created by or, min, max, abs, etc.)
    local_types: HashMap<u32, Type>,
    /// Next local index
    next_local: u32,
    /// Function index map (indices start after imports)
    func_indices: HashMap<String, u32>,
    /// Import index map: (alias, function_name) -> import index
    import_indices: HashMap<(String, String), u32>,
    /// Number of imports (for calculating function indices)
    num_imports: u32,
    /// Whether current expression is in tail position (for TCO)
    in_tail_position: bool,
    /// Current loop binding local indices (for recur)
    loop_binding_locals: Vec<u32>,
}

impl Lowerer {
    fn new() -> Self {
        Self {
            module: Module::new(),
            locals: HashMap::new(),
            local_types: HashMap::new(),
            next_local: 0,
            func_indices: HashMap::new(),
            import_indices: HashMap::new(),
            num_imports: 0,
            in_tail_position: false,
            loop_binding_locals: Vec::new(),
        }
    }

    fn lower_module(&mut self, analyzed: &AnalyzedModule) -> CompileResult<Module> {
        // Lower imports first - they occupy the first function indices
        for (idx, import) in analyzed.imports.iter().enumerate() {
            self.import_indices.insert(
                (import.alias.clone(), import.function_name.clone()),
                idx as u32,
            );
            self.module.imports.push(Import {
                local_name: import.alias.clone(),
                wit_interface: import.wit_interface.clone(),
                function_name: import.function_name.clone(),
                params: import.params.clone(),
                return_type: import.return_type.clone(),
            });
        }
        self.num_imports = analyzed.imports.len() as u32;

        // Build function index map - indices start after imports
        for (idx, func) in analyzed.functions.iter().enumerate() {
            self.func_indices.insert(func.name.clone(), self.num_imports + idx as u32);
        }

        // Lower globals
        for global in &analyzed.globals {
            let lowered = self.lower_global(global)?;
            self.module.globals.push(lowered);
        }

        // Lower functions
        for func in &analyzed.functions {
            let lowered = self.lower_function(func)?;
            self.module.functions.push(lowered);
        }

        Ok(std::mem::take(&mut self.module))
    }

    fn lower_global(&mut self, global: &AnalyzedGlobal) -> CompileResult<Global> {
        let init = self.lower_expr(&global.init)?;
        Ok(Global {
            name: global.name.clone(),
            ty: global.ty.clone(),
            init,
        })
    }

    fn lower_function(&mut self, func: &AnalyzedFunction) -> CompileResult<Function> {
        // Reset locals for new function
        self.locals.clear();
        self.local_types.clear();
        self.next_local = 0;

        // Add parameters as locals with their types
        let mut params = Vec::new();
        for (name, ty) in &func.params {
            let idx = self.next_local;
            self.locals.insert(name.clone(), (idx, ty.clone()));
            self.local_types.insert(idx, ty.clone());
            self.next_local += 1;
            params.push((name.clone(), ty.clone()));
        }

        // Lower body - function body is in tail position
        self.in_tail_position = true;
        let body = self.lower_expr(&func.body)?;
        self.in_tail_position = false;

        // Collect local types from tracked info
        let mut locals = vec![Type::Unknown; self.next_local as usize];
        for (&idx, ty) in &self.local_types {
            locals[idx as usize] = ty.clone();
        }

        Ok(Function {
            name: func.name.clone(),
            exported: func.exported,
            export_name: func.export_name.clone(),
            params,
            return_type: func.return_type.clone(),
            locals,
            body,
        })
    }

    fn lower_expr(&mut self, expr: &Edn) -> CompileResult<Expr> {
        match expr {
            Edn::Nil => Ok(Expr::Unit),

            Edn::Bool(b) => Ok(Expr::Bool(*b)),

            Edn::Number(n) => {
                use suss_core::Number;
                match n {
                    Number::Integer(i) => {
                        // Try to convert to i64
                        let val: i64 = i.try_into().map_err(|_| {
                            CompileError::Unsupported("Integer too large for i64".into())
                        })?;
                        Ok(Expr::Int(val))
                    }
                    Number::Float(f) => Ok(Expr::Float(*f)),
                    Number::Ratio(_) => {
                        Err(CompileError::Unsupported("Ratios not supported".into()))
                    }
                }
            }

            Edn::String(s) => {
                let idx = self.module.intern_string(s);
                Ok(Expr::String(idx))
            }

            Edn::Char(c) => Ok(Expr::Int(*c as i64)),

            Edn::Symbol(sym) => {
                // Check if it's a local
                if let Some(&(idx, ref ty)) = self.locals.get(&sym.name) {
                    Ok(Expr::LocalGet { local: idx, ty: ty.clone() })
                } else {
                    Err(CompileError::Undefined(sym.name.clone()))
                }
            }

            Edn::List(items) if items.is_empty() => Ok(Expr::Unit),

            Edn::List(items) => {
                if let Edn::Symbol(sym) = &items[0] {
                    // Check if it's a namespaced symbol (e.g., wasi.random/get-random-u64)
                    if let Some(ns) = &sym.namespace {
                        // Reconstruct the full qualified name
                        let full_name = format!("{}/{}", ns, sym.name);
                        self.lower_call(&full_name, &items[1..])
                    } else {
                        self.lower_call(&sym.name, &items[1..])
                    }
                } else {
                    // Function expression call
                    Err(CompileError::Unsupported(
                        "Function expression calls not yet supported".into()
                    ))
                }
            }

            Edn::Vector(items) => {
                // GC mode: create persistent vector
                let elements: Vec<Expr> = items
                    .iter()
                    .map(|item| self.lower_expr(item))
                    .collect::<Result<_, _>>()?;
                Ok(Expr::VecNew(elements))
            }

            Edn::Map(pairs) => {
                // GC mode: create persistent map
                let lowered_pairs: Vec<(Expr, Expr)> = pairs
                    .iter()
                    .map(|(k, v)| Ok((self.lower_expr(k)?, self.lower_expr(v)?)))
                    .collect::<Result<_, CompileError>>()?;
                Ok(Expr::MapNew(lowered_pairs))
            }

            Edn::Set(items) => {
                // GC mode: create persistent set
                let elements: Vec<Expr> = items
                    .iter()
                    .map(|item| self.lower_expr(item))
                    .collect::<Result<_, _>>()?;
                Ok(Expr::SetNew(elements))
            }

            _ => Err(CompileError::Unsupported(format!(
                "Expression type not supported: {:?}",
                expr
            ))),
        }
    }

    /// Infer the numeric type from operands
    fn infer_numeric_type(&self, args: &[Edn]) -> Type {
        if args.is_empty() {
            return Type::I32;
        }
        // Check all arguments for floats - if any is float, use F64
        for arg in args {
            if let Edn::Number(Number::Float(_)) = arg {
                return Type::F64;
            }
        }
        // Check if any integer is too large for i32
        for arg in args {
            if let Edn::Number(Number::Integer(i)) = arg {
                if let Some(v) = i.to_i64() {
                    if v < i32::MIN as i64 || v > i32::MAX as i64 {
                        return Type::I64;
                    }
                } else {
                    return Type::I64;
                }
            }
        }
        Type::I32
    }

    fn lower_call(&mut self, name: &str, args: &[Edn]) -> CompileResult<Expr> {
        match name {
            // Arithmetic - infer type from operands
            "+" => {
                let ty = self.infer_numeric_type(args);
                self.lower_binop_chain(BinOp::Add, args, ty)
            }
            "-" => {
                let ty = self.infer_numeric_type(args);
                self.lower_binop_chain(BinOp::Sub, args, ty)
            }
            "*" => {
                let ty = self.infer_numeric_type(args);
                self.lower_binop_chain(BinOp::Mul, args, ty)
            }
            "/" => {
                let ty = self.infer_numeric_type(args);
                self.lower_binop_chain(BinOp::Div, args, ty)
            }
            "rem" | "mod" => {
                let ty = self.infer_numeric_type(args);
                self.lower_binop(BinOp::Rem, args, ty)
            }

            // Numeric operations (desugar to primitives)
            "inc" => self.lower_inc(args),
            "dec" => self.lower_dec(args),
            "abs" => self.lower_abs(args),
            "min" => self.lower_min(args),
            "max" => self.lower_max(args),

            // Comparison
            "=" => self.lower_binop(BinOp::Eq, args, Type::Bool),
            "not=" => self.lower_binop(BinOp::Ne, args, Type::Bool),
            "<" => self.lower_binop(BinOp::Lt, args, Type::Bool),
            "<=" => self.lower_binop(BinOp::Le, args, Type::Bool),
            ">" => self.lower_binop(BinOp::Gt, args, Type::Bool),
            ">=" => self.lower_binop(BinOp::Ge, args, Type::Bool),

            // Logical (short-circuit)
            "and" => self.lower_and(args),
            "or" => self.lower_or(args),
            "not" => {
                if args.len() != 1 {
                    return Err(CompileError::Parse("not requires exactly 1 argument".into()));
                }
                let operand = self.lower_expr(&args[0])?;
                Ok(Expr::UnOp {
                    op: UnOp::Not,
                    operand: Box::new(operand),
                    ty: Type::Bool,
                })
            }

            // Control flow
            "if" => self.lower_if(args),
            "do" => self.lower_do(args),
            "let" => self.lower_let(args),
            "loop" => self.lower_loop(args),
            "recur" => self.lower_recur(args),
            "cond" => self.lower_cond(args),
            "when" => self.lower_when(args),
            "when-not" => self.lower_when_not(args),
            "case" => self.lower_case(args),

            // String
            "str" => self.lower_str(args),

            // Collection operations
            "nth" => self.lower_nth(args),
            "first" => self.lower_first(args),
            "rest" => self.lower_rest(args),
            "conj" => self.lower_conj(args),
            "cons" => self.lower_cons(args),
            "count" => self.lower_count(args),
            "get" => self.lower_get(args),
            "assoc" => self.lower_assoc(args),
            "contains?" => self.lower_contains(args),

            // Hashing
            "hash" => self.lower_hash(args),

            // Function call
            _ => self.lower_func_call(name, args),
        }
    }

    fn lower_binop(&mut self, op: BinOp, args: &[Edn], ty: Type) -> CompileResult<Expr> {
        if args.len() != 2 {
            return Err(CompileError::Parse(format!(
                "Binary operator requires exactly 2 arguments, got {}",
                args.len()
            )));
        }

        // Arguments to binary operations are never in tail position
        let was_tail = self.in_tail_position;
        self.in_tail_position = false;

        let left = self.lower_expr(&args[0])?;
        let right = self.lower_expr(&args[1])?;

        self.in_tail_position = was_tail;
        Ok(Expr::BinOp {
            op,
            left: Box::new(left),
            right: Box::new(right),
            ty,
        })
    }

    fn lower_binop_chain(&mut self, op: BinOp, args: &[Edn], ty: Type) -> CompileResult<Expr> {
        if args.is_empty() {
            return Err(CompileError::Parse("Operator requires at least 1 argument".into()));
        }
        if args.len() == 1 {
            return self.lower_expr(&args[0]);
        }

        // Arguments to binary operations are never in tail position
        let was_tail = self.in_tail_position;
        self.in_tail_position = false;

        let mut result = self.lower_expr(&args[0])?;
        for arg in &args[1..] {
            let right = self.lower_expr(arg)?;
            result = Expr::BinOp {
                op,
                left: Box::new(result),
                right: Box::new(right),
                ty: ty.clone(),
            };
        }

        self.in_tail_position = was_tail;
        Ok(result)
    }

    fn lower_if(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() < 2 {
            return Err(CompileError::Parse("if requires condition and then branch".into()));
        }

        // Condition is never in tail position
        let was_tail = self.in_tail_position;
        self.in_tail_position = false;
        let cond = self.lower_expr(&args[0])?;

        // Both branches inherit parent's tail position
        self.in_tail_position = was_tail;
        let then_branch = self.lower_expr(&args[1])?;
        let else_branch = if args.len() > 2 {
            self.lower_expr(&args[2])?
        } else {
            Expr::Unit
        };

        Ok(Expr::If {
            cond: Box::new(cond),
            then_branch: Box::new(then_branch),
            else_branch: Box::new(else_branch),
            ty: Type::Unknown, // Will be inferred
        })
    }

    fn lower_do(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.is_empty() {
            return Ok(Expr::Block(vec![]));
        }

        let was_tail = self.in_tail_position;
        let mut exprs = Vec::with_capacity(args.len());

        // All but the last expression are NOT in tail position
        for arg in &args[..args.len() - 1] {
            self.in_tail_position = false;
            exprs.push(self.lower_expr(arg)?);
        }

        // Last expression inherits tail position
        self.in_tail_position = was_tail;
        exprs.push(self.lower_expr(args.last().unwrap())?);

        Ok(Expr::Block(exprs))
    }

    fn lower_let(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.is_empty() {
            return Err(CompileError::Parse("let requires bindings".into()));
        }

        // Parse bindings vector
        let bindings_vec = match &args[0] {
            Edn::Vector(items) => items,
            _ => return Err(CompileError::Parse("let bindings must be a vector".into())),
        };

        if bindings_vec.len() % 2 != 0 {
            return Err(CompileError::Parse("let bindings must have even number of forms".into()));
        }

        // Bindings are NOT in tail position
        let was_tail = self.in_tail_position;
        self.in_tail_position = false;

        let mut bindings = Vec::new();
        for chunk in bindings_vec.chunks(2) {
            let name = match &chunk[0] {
                Edn::Symbol(sym) => sym.name.clone(),
                _ => return Err(CompileError::Parse("let binding name must be a symbol".into())),
            };

            let value = self.lower_expr(&chunk[1])?;
            let value_ty = value.expr_type();
            let idx = self.next_local;
            self.locals.insert(name, (idx, value_ty.clone()));
            self.local_types.insert(idx, value_ty);
            self.next_local += 1;
            bindings.push((idx, value));
        }

        // Body inherits tail position
        self.in_tail_position = was_tail;
        let body = if args.len() > 1 {
            if args.len() == 2 {
                self.lower_expr(&args[1])?
            } else {
                self.lower_do(&args[1..])?
            }
        } else {
            Expr::Unit
        };

        Ok(Expr::Let {
            bindings,
            body: Box::new(body),
        })
    }

    fn lower_loop(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.is_empty() {
            return Err(CompileError::Parse("loop requires bindings".into()));
        }

        // Parse bindings (same as let)
        let bindings_vec = match &args[0] {
            Edn::Vector(items) => items,
            _ => return Err(CompileError::Parse("loop bindings must be a vector".into())),
        };

        if bindings_vec.len() % 2 != 0 {
            return Err(CompileError::Parse("loop bindings must have even number of forms".into()));
        }

        // Neither bindings nor body are in tail position for loop
        // (recur handles the internal loop, TCO is for function-level recursion)
        let was_tail = self.in_tail_position;
        self.in_tail_position = false;

        // Save old loop bindings for nested loops
        let old_loop_bindings = std::mem::take(&mut self.loop_binding_locals);

        let mut bindings = Vec::new();
        let mut binding_locals = Vec::new();
        for chunk in bindings_vec.chunks(2) {
            let name = match &chunk[0] {
                Edn::Symbol(sym) => sym.name.clone(),
                _ => return Err(CompileError::Parse("loop binding name must be a symbol".into())),
            };

            let value = self.lower_expr(&chunk[1])?;
            let value_ty = value.expr_type();
            let idx = self.next_local;
            self.locals.insert(name, (idx, value_ty.clone()));
            self.local_types.insert(idx, value_ty);
            self.next_local += 1;
            bindings.push((idx, value));
            binding_locals.push(idx);
        }

        // Set loop binding locals for recur to use
        self.loop_binding_locals = binding_locals;

        // Lower body (not in tail position - loop uses WASM loop/br, not return_call)
        let body = if args.len() > 1 {
            if args.len() == 2 {
                self.lower_expr(&args[1])?
            } else {
                self.lower_do(&args[1..])?
            }
        } else {
            Expr::Unit
        };

        // Restore old loop bindings and tail position
        self.loop_binding_locals = old_loop_bindings;
        self.in_tail_position = was_tail;

        Ok(Expr::Loop {
            bindings,
            body: Box::new(body),
        })
    }

    fn lower_recur(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != self.loop_binding_locals.len() {
            return Err(CompileError::Parse(format!(
                "recur requires {} arguments, got {}",
                self.loop_binding_locals.len(),
                args.len()
            )));
        }

        // Clone the local indices to avoid borrow conflict
        let local_indices: Vec<u32> = self.loop_binding_locals.clone();

        let mut values = Vec::new();
        for (e, local_idx) in args.iter().zip(local_indices.iter()) {
            let expr = self.lower_expr(e)?;
            values.push((*local_idx, expr));
        }
        Ok(Expr::Recur(values))
    }

    // ========================================================================
    // Numeric operations (desugar to primitives)
    // ========================================================================

    /// (inc n) -> (+ n 1)
    fn lower_inc(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 1 {
            return Err(CompileError::Parse("inc requires exactly 1 argument".into()));
        }
        // Arguments are never in tail position
        let was_tail = self.in_tail_position;
        self.in_tail_position = false;
        let n = self.lower_expr(&args[0])?;
        self.in_tail_position = was_tail;
        let ty = n.expr_type();
        let one = match &ty {
            Type::I64 => Expr::Int(1),
            Type::F64 => Expr::Float(1.0),
            _ => Expr::Int(1),
        };
        Ok(Expr::BinOp {
            op: BinOp::Add,
            left: Box::new(n),
            right: Box::new(one),
            ty: if ty == Type::Unknown { Type::I32 } else { ty },
        })
    }

    /// (dec n) -> (- n 1)
    fn lower_dec(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 1 {
            return Err(CompileError::Parse("dec requires exactly 1 argument".into()));
        }
        // Arguments are never in tail position
        let was_tail = self.in_tail_position;
        self.in_tail_position = false;
        let n = self.lower_expr(&args[0])?;
        self.in_tail_position = was_tail;
        let ty = n.expr_type();
        let one = match &ty {
            Type::I64 => Expr::Int(1),
            Type::F64 => Expr::Float(1.0),
            _ => Expr::Int(1),
        };
        Ok(Expr::BinOp {
            op: BinOp::Sub,
            left: Box::new(n),
            right: Box::new(one),
            ty: if ty == Type::Unknown { Type::I32 } else { ty },
        })
    }

    /// (abs n) -> (if (< n 0) (- 0 n) n)
    fn lower_abs(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 1 {
            return Err(CompileError::Parse("abs requires exactly 1 argument".into()));
        }
        // Arguments are never in tail position
        let was_tail = self.in_tail_position;
        self.in_tail_position = false;
        let n = self.lower_expr(&args[0])?;
        self.in_tail_position = was_tail;
        let ty = n.expr_type();
        let result_ty = if ty == Type::Unknown { Type::I32 } else { ty.clone() };

        // Store n in a local to avoid double evaluation
        let n_local = self.next_local;
        self.local_types.insert(n_local, result_ty.clone());
        self.next_local += 1;

        let zero = match &result_ty {
            Type::I64 => Expr::Int(0),
            Type::F64 => Expr::Float(0.0),
            _ => Expr::Int(0),
        };

        Ok(Expr::Let {
            bindings: vec![(n_local, n)],
            body: Box::new(Expr::If {
                cond: Box::new(Expr::BinOp {
                    op: BinOp::Lt,
                    left: Box::new(Expr::LocalGet { local: n_local, ty: result_ty.clone() }),
                    right: Box::new(zero.clone()),
                    ty: Type::Bool,
                }),
                then_branch: Box::new(Expr::BinOp {
                    op: BinOp::Sub,
                    left: Box::new(zero),
                    right: Box::new(Expr::LocalGet { local: n_local, ty: result_ty.clone() }),
                    ty: result_ty.clone(),
                }),
                else_branch: Box::new(Expr::LocalGet { local: n_local, ty: result_ty.clone() }),
                ty: result_ty,
            }),
        })
    }

    /// (min a b ...) -> nested comparisons returning smallest
    fn lower_min(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.is_empty() {
            return Err(CompileError::Parse("min requires at least 1 argument".into()));
        }
        if args.len() == 1 {
            return self.lower_expr(&args[0]);
        }

        // Arguments are never in tail position
        let was_tail = self.in_tail_position;
        self.in_tail_position = false;

        // For 2+ args: (min a b c) -> (let [t1 a t2 b] (if (< t1 t2) (min t1 c) (min t2 c)))
        // Simplified: chain of binary comparisons
        let mut result = self.lower_expr(&args[0])?;
        let ty = result.expr_type();
        let result_ty = if ty == Type::Unknown { Type::I32 } else { ty };

        for arg in &args[1..] {
            let b = self.lower_expr(arg)?;

            // Store both in locals to avoid double evaluation
            let a_local = self.next_local;
            self.local_types.insert(a_local, result_ty.clone());
            self.next_local += 1;
            let b_local = self.next_local;
            self.local_types.insert(b_local, result_ty.clone());
            self.next_local += 1;

            result = Expr::Let {
                bindings: vec![(a_local, result), (b_local, b)],
                body: Box::new(Expr::If {
                    cond: Box::new(Expr::BinOp {
                        op: BinOp::Lt,
                        left: Box::new(Expr::LocalGet { local: a_local, ty: result_ty.clone() }),
                        right: Box::new(Expr::LocalGet { local: b_local, ty: result_ty.clone() }),
                        ty: Type::Bool,
                    }),
                    then_branch: Box::new(Expr::LocalGet { local: a_local, ty: result_ty.clone() }),
                    else_branch: Box::new(Expr::LocalGet { local: b_local, ty: result_ty.clone() }),
                    ty: result_ty.clone(),
                }),
            };
        }

        self.in_tail_position = was_tail;
        Ok(result)
    }

    /// (max a b ...) -> nested comparisons returning largest
    fn lower_max(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.is_empty() {
            return Err(CompileError::Parse("max requires at least 1 argument".into()));
        }
        if args.len() == 1 {
            return self.lower_expr(&args[0]);
        }

        // Arguments are never in tail position
        let was_tail = self.in_tail_position;
        self.in_tail_position = false;

        let mut result = self.lower_expr(&args[0])?;
        let ty = result.expr_type();
        let result_ty = if ty == Type::Unknown { Type::I32 } else { ty };

        for arg in &args[1..] {
            let b = self.lower_expr(arg)?;

            let a_local = self.next_local;
            self.local_types.insert(a_local, result_ty.clone());
            self.next_local += 1;
            let b_local = self.next_local;
            self.local_types.insert(b_local, result_ty.clone());
            self.next_local += 1;

            result = Expr::Let {
                bindings: vec![(a_local, result), (b_local, b)],
                body: Box::new(Expr::If {
                    cond: Box::new(Expr::BinOp {
                        op: BinOp::Gt,  // Changed from Lt to Gt for max
                        left: Box::new(Expr::LocalGet { local: a_local, ty: result_ty.clone() }),
                        right: Box::new(Expr::LocalGet { local: b_local, ty: result_ty.clone() }),
                        ty: Type::Bool,
                    }),
                    then_branch: Box::new(Expr::LocalGet { local: a_local, ty: result_ty.clone() }),
                    else_branch: Box::new(Expr::LocalGet { local: b_local, ty: result_ty.clone() }),
                    ty: result_ty.clone(),
                }),
            };
        }

        self.in_tail_position = was_tail;
        Ok(result)
    }

    /// (cond test1 expr1 test2 expr2 ... :else default)
    /// Desugars to: (if test1 expr1 (if test2 expr2 (... default)))
    fn lower_cond(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.is_empty() {
            return Ok(Expr::Unit);
        }

        let was_tail = self.in_tail_position;

        // Build nested if from end to start
        let mut result = Expr::Unit;
        let mut i = args.len();

        // Handle odd number of args (trailing else value)
        // This is the final else, inherits tail position
        if i % 2 == 1 {
            i -= 1;
            self.in_tail_position = was_tail;
            result = self.lower_expr(&args[i])?;
        }

        // Process pairs in reverse: (test expr)
        while i >= 2 {
            i -= 2;
            let test = &args[i];
            let expr = &args[i + 1];

            // Condition is never in tail position
            self.in_tail_position = false;
            let cond = if let Edn::Keyword(k) = test {
                if k.name == "else" {
                    Expr::Bool(true)
                } else {
                    self.lower_expr(test)?
                }
            } else {
                self.lower_expr(test)?
            };

            // Result expression inherits tail position
            self.in_tail_position = was_tail;
            result = Expr::If {
                cond: Box::new(cond),
                then_branch: Box::new(self.lower_expr(expr)?),
                else_branch: Box::new(result),
                ty: Type::Unknown,
            };
        }

        Ok(result)
    }

    /// (when test body...) -> (if test (do body...) nil)
    fn lower_when(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.is_empty() {
            return Err(CompileError::Parse("when requires a test".into()));
        }

        let was_tail = self.in_tail_position;

        // Condition is never in tail position
        self.in_tail_position = false;
        let cond = self.lower_expr(&args[0])?;

        // Body inherits tail position
        self.in_tail_position = was_tail;
        let body = if args.len() == 2 {
            self.lower_expr(&args[1])?
        } else if args.len() > 2 {
            self.lower_do(&args[1..])?
        } else {
            Expr::Unit
        };

        Ok(Expr::If {
            cond: Box::new(cond),
            then_branch: Box::new(body),
            else_branch: Box::new(Expr::Unit),
            ty: Type::Unknown,
        })
    }

    /// (when-not test body...) -> (if test nil (do body...))
    fn lower_when_not(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.is_empty() {
            return Err(CompileError::Parse("when-not requires a test".into()));
        }

        let was_tail = self.in_tail_position;

        // Condition is never in tail position
        self.in_tail_position = false;
        let cond = self.lower_expr(&args[0])?;

        // Body inherits tail position
        self.in_tail_position = was_tail;
        let body = if args.len() == 2 {
            self.lower_expr(&args[1])?
        } else if args.len() > 2 {
            self.lower_do(&args[1..])?
        } else {
            Expr::Unit
        };

        Ok(Expr::If {
            cond: Box::new(cond),
            then_branch: Box::new(Expr::Unit),
            else_branch: Box::new(body),
            ty: Type::Unknown,
        })
    }

    /// (case expr val1 result1 val2 result2 ... default)
    /// Desugars to: (let [v expr] (if (= v val1) result1 (if (= v val2) result2 ... default)))
    fn lower_case(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.is_empty() {
            return Err(CompileError::Parse("case requires an expression".into()));
        }

        let was_tail = self.in_tail_position;

        // Test expression is not in tail position
        self.in_tail_position = false;
        let test_expr = self.lower_expr(&args[0])?;
        let test_ty = test_expr.expr_type();
        let test_local = self.next_local;
        self.local_types.insert(test_local, test_ty.clone());
        self.next_local += 1;

        let cases = &args[1..];
        let mut result = Expr::Unit;
        let mut i = cases.len();

        // Handle odd (default value) - inherits tail position
        if i % 2 == 1 {
            i -= 1;
            self.in_tail_position = was_tail;
            result = self.lower_expr(&cases[i])?;
        }

        // Build nested if from end
        while i >= 2 {
            i -= 2;
            // Case values are not in tail position
            self.in_tail_position = false;
            let val = self.lower_expr(&cases[i])?;
            // Result expressions inherit tail position
            self.in_tail_position = was_tail;
            let then_expr = self.lower_expr(&cases[i + 1])?;

            result = Expr::If {
                cond: Box::new(Expr::BinOp {
                    op: BinOp::Eq,
                    left: Box::new(Expr::LocalGet { local: test_local, ty: test_ty.clone() }),
                    right: Box::new(val),
                    ty: Type::Bool,
                }),
                then_branch: Box::new(then_expr),
                else_branch: Box::new(result),
                ty: Type::Unknown,
            };
        }

        Ok(Expr::Let {
            bindings: vec![(test_local, test_expr)],
            body: Box::new(result),
        })
    }

    /// (and a b c ...) - short-circuit evaluation
    /// Returns the first falsy value, or the last value if all are truthy.
    /// (and) -> true
    /// (and a) -> a
    /// (and a b) -> (if a b false)
    /// (and a b c) -> (if a (if b c false) false)
    fn lower_and(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.is_empty() {
            return Ok(Expr::Bool(true));
        }
        if args.len() == 1 {
            return self.lower_expr(&args[0]);
        }

        let was_tail = self.in_tail_position;

        // Build from right to left: (if a (if b c false) false)
        // Last value inherits tail position
        self.in_tail_position = was_tail;
        let mut result = self.lower_expr(args.last().unwrap())?;

        // Conditions are not in tail position, but the nested result (which is already built) is
        for arg in args[..args.len() - 1].iter().rev() {
            self.in_tail_position = false;
            let cond = self.lower_expr(arg)?;
            result = Expr::If {
                cond: Box::new(cond),
                then_branch: Box::new(result),
                else_branch: Box::new(Expr::Bool(false)),
                ty: Type::Unknown,
            };
        }

        Ok(result)
    }

    /// (or a b c ...) - short-circuit evaluation
    /// Returns the first truthy value, or the last value if all are falsy.
    /// (or) -> nil/false
    /// (or a) -> a
    /// (or a b) -> (let [t a] (if t t b))
    /// (or a b c) -> (let [t a] (if t t (let [t b] (if t t c))))
    fn lower_or(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.is_empty() {
            return Ok(Expr::Bool(false));
        }
        if args.len() == 1 {
            return self.lower_expr(&args[0]);
        }

        let was_tail = self.in_tail_position;

        // Build from right to left
        // Each value needs to be bound to a temp to avoid double evaluation
        // Last value inherits tail position
        self.in_tail_position = was_tail;
        let mut result = self.lower_expr(args.last().unwrap())?;

        // All values being tested are not in tail position (they're conditions/bindings)
        for arg in args[..args.len() - 1].iter().rev() {
            self.in_tail_position = false;
            let val = self.lower_expr(arg)?;
            let val_ty = val.expr_type();
            let temp_local = self.next_local;
            self.local_types.insert(temp_local, val_ty.clone());
            self.next_local += 1;

            // (let [t val] (if t t result))
            // The then_branch (LocalGet) is not a tail call, but result may contain one
            result = Expr::Let {
                bindings: vec![(temp_local, val)],
                body: Box::new(Expr::If {
                    cond: Box::new(Expr::LocalGet { local: temp_local, ty: val_ty.clone() }),
                    then_branch: Box::new(Expr::LocalGet { local: temp_local, ty: val_ty }),
                    else_branch: Box::new(result),
                    ty: Type::Unknown,
                }),
            };
        }

        Ok(result)
    }

    fn lower_str(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        // First try compile-time concatenation for all-literal strings
        let all_literals = args.iter().all(|arg| matches!(arg, Edn::String(_)));

        if all_literals && !args.is_empty() {
            // Concatenate at compile time
            let mut result = String::new();
            for arg in args {
                if let Edn::String(s) = arg {
                    result.push_str(s);
                }
            }
            let idx = self.module.intern_string(&result);
            return Ok(Expr::String(idx));
        }

        // Fall back to runtime concatenation
        // Arguments are never in tail position
        let was_tail = self.in_tail_position;
        self.in_tail_position = false;
        let parts: Vec<Expr> = args
            .iter()
            .map(|e| self.lower_expr(e))
            .collect::<CompileResult<_>>()?;
        self.in_tail_position = was_tail;
        Ok(Expr::StrConcat(parts))
    }

    fn lower_func_call(&mut self, name: &str, args: &[Edn]) -> CompileResult<Expr> {
        // Check if it's a qualified import call (e.g., "random/get-random-u64" or "wasi.random/get-random-u64")
        if let Some(slash_pos) = name.find('/') {
            let alias = &name[..slash_pos];
            let func_name = &name[slash_pos + 1..];

            // Look up in import indices
            if let Some(&idx) = self.import_indices.get(&(alias.to_string(), func_name.to_string())) {
                // Arguments are never in tail position
                let was_tail = self.in_tail_position;
                self.in_tail_position = false;
                let lowered_args: Vec<Expr> = args
                    .iter()
                    .map(|e| self.lower_expr(e))
                    .collect::<CompileResult<_>>()?;
                self.in_tail_position = was_tail;

                // Imports can be tail calls too
                return if self.in_tail_position {
                    Ok(Expr::TailCall {
                        func: idx,
                        args: lowered_args,
                    })
                } else {
                    Ok(Expr::Call {
                        func: idx,
                        args: lowered_args,
                    })
                };
            }

            // Not found in imports - if this is a wasi.* call, provide helpful error
            if alias.starts_with("wasi.") {
                return Err(CompileError::Undefined(format!(
                    "WASI function '{}' not found. Make sure the function is supported. \
                     Available: wasi.random/get-random-u64",
                    name
                )));
            } else {
                return Err(CompileError::Undefined(format!(
                    "Import function '{}' not found (alias: '{}', function: '{}')",
                    name, alias, func_name
                )));
            }
        }

        // Check if it's a direct :refer import (no alias)
        if let Some(&idx) = self.import_indices.get(&(String::new(), name.to_string())) {
            // Arguments are never in tail position
            let was_tail = self.in_tail_position;
            self.in_tail_position = false;
            let lowered_args: Vec<Expr> = args
                .iter()
                .map(|e| self.lower_expr(e))
                .collect::<CompileResult<_>>()?;
            self.in_tail_position = was_tail;

            return if self.in_tail_position {
                Ok(Expr::TailCall {
                    func: idx,
                    args: lowered_args,
                })
            } else {
                Ok(Expr::Call {
                    func: idx,
                    args: lowered_args,
                })
            };
        }

        // Look up local function by name
        let func_idx = self.func_indices.get(name).copied();

        if let Some(idx) = func_idx {
            // Arguments are never in tail position
            let was_tail = self.in_tail_position;
            self.in_tail_position = false;
            let lowered_args: Vec<Expr> = args
                .iter()
                .map(|e| self.lower_expr(e))
                .collect::<CompileResult<_>>()?;
            self.in_tail_position = was_tail;

            if self.in_tail_position {
                Ok(Expr::TailCall {
                    func: idx,
                    args: lowered_args,
                })
            } else {
                Ok(Expr::Call {
                    func: idx,
                    args: lowered_args,
                })
            }
        } else {
            Err(CompileError::Undefined(name.to_string()))
        }
    }

    // ========================================================================
    // Collection Operations
    // ========================================================================

    /// Infer the collection type of an expression at compile time.
    ///
    /// Returns Some(type_id) if the type is statically known, None otherwise.
    /// Used for fast-path dispatch optimization.
    ///
    /// Note: Edn::List is NOT included here because in Clojure/Suss syntax,
    /// parentheses `(...)` denote function calls, not list literals. The expression
    /// `(let [v []] v)` parses as Edn::List but is a function call that returns
    /// whatever type its body returns. Only literal syntax ([], {}, #{}) can be
    /// reliably inferred at the EDN level.
    fn infer_collection_type(&self, expr: &Edn) -> Option<u32> {
        use crate::ir::gc_types;
        match expr {
            Edn::Vector(_) => Some(gc_types::PERSISTENT_VECTOR),
            Edn::Map(_) => Some(gc_types::PERSISTENT_MAP),
            Edn::Set(_) => Some(gc_types::PERSISTENT_SET),
            // Edn::List is intentionally NOT matched here - see docstring above
            _ => None,
        }
    }

    /// Lower (nth coll index) -> VecNth or ProtocolDispatch
    ///
    /// If the collection type is known at compile time, uses the fast path.
    /// Otherwise, falls back to runtime protocol dispatch.
    fn lower_nth(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        use crate::ir::{gc_types, method_ids};

        if args.len() != 2 {
            return Err(CompileError::Parse(
                "nth requires exactly 2 arguments: collection and index".into(),
            ));
        }

        // Arguments are never in tail position
        let was_tail = self.in_tail_position;
        self.in_tail_position = false;
        let coll = self.lower_expr(&args[0])?;
        let index = self.lower_expr(&args[1])?;
        self.in_tail_position = was_tail;

        // Fast path: if we know the collection type at compile time
        if let Some(type_id) = self.infer_collection_type(&args[0]) {
            if type_id == gc_types::PERSISTENT_VECTOR {
                return Ok(Expr::VecNth {
                    vec: Box::new(coll),
                    index: Box::new(index),
                });
            }
            // For other collection types, we would add specific Expr variants here
            // For now, fall through to protocol dispatch
        }

        // Slow path: runtime protocol dispatch
        Ok(Expr::ProtocolDispatch {
            obj: Box::new(coll),
            method_id: method_ids::NTH,
            args: vec![index],
            in_tail_position: self.in_tail_position,
        })
    }

    /// Lower (first coll) -> ListFirst or ProtocolDispatch
    ///
    /// If the collection type is known to be a list at compile time, uses ListFirst.
    /// Otherwise, falls back to runtime protocol dispatch.
    fn lower_first(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        use crate::ir::{gc_types, method_ids};

        if args.len() != 1 {
            return Err(CompileError::Parse(
                "first requires exactly 1 argument".into(),
            ));
        }

        // Arguments are never in tail position
        let was_tail = self.in_tail_position;
        self.in_tail_position = false;
        let coll = self.lower_expr(&args[0])?;
        self.in_tail_position = was_tail;

        // Fast path: if we know the collection is a CONS (list)
        if let Some(type_id) = self.infer_collection_type(&args[0]) {
            if type_id == gc_types::CONS {
                return Ok(Expr::ListFirst(Box::new(coll)));
            }
        }

        // Slow path: runtime protocol dispatch
        Ok(Expr::ProtocolDispatch {
            obj: Box::new(coll),
            method_id: method_ids::FIRST,
            args: vec![],
            in_tail_position: self.in_tail_position,
        })
    }

    /// Lower (rest coll) -> ListRest or ProtocolDispatch
    ///
    /// If the collection type is known to be a list at compile time, uses ListRest.
    /// Otherwise, falls back to runtime protocol dispatch.
    fn lower_rest(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        use crate::ir::{gc_types, method_ids};

        if args.len() != 1 {
            return Err(CompileError::Parse(
                "rest requires exactly 1 argument".into(),
            ));
        }

        // Arguments are never in tail position
        let was_tail = self.in_tail_position;
        self.in_tail_position = false;
        let coll = self.lower_expr(&args[0])?;
        self.in_tail_position = was_tail;

        // Fast path: if we know the collection is a CONS (list)
        if let Some(type_id) = self.infer_collection_type(&args[0]) {
            if type_id == gc_types::CONS {
                return Ok(Expr::ListRest(Box::new(coll)));
            }
        }

        // Slow path: runtime protocol dispatch
        Ok(Expr::ProtocolDispatch {
            obj: Box::new(coll),
            method_id: method_ids::REST,
            args: vec![],
            in_tail_position: self.in_tail_position,
        })
    }

    /// Lower (conj coll val) -> VecConj, SetConj, Cons, or ProtocolDispatch
    ///
    /// If the collection type is known at compile time, uses the appropriate fast path.
    /// Otherwise, falls back to runtime protocol dispatch.
    fn lower_conj(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        use crate::ir::{gc_types, method_ids};

        if args.len() != 2 {
            return Err(CompileError::Parse(
                "conj requires exactly 2 arguments: collection and value".into(),
            ));
        }

        // Arguments are never in tail position
        let was_tail = self.in_tail_position;
        self.in_tail_position = false;
        let coll = self.lower_expr(&args[0])?;
        let val = self.lower_expr(&args[1])?;
        self.in_tail_position = was_tail;

        // Fast path: if we know the collection type at compile time
        if let Some(type_id) = self.infer_collection_type(&args[0]) {
            return match type_id {
                t if t == gc_types::PERSISTENT_VECTOR => Ok(Expr::VecConj {
                    vec: Box::new(coll),
                    val: Box::new(val),
                }),
                t if t == gc_types::PERSISTENT_SET => Ok(Expr::SetConj {
                    set: Box::new(coll),
                    val: Box::new(val),
                }),
                t if t == gc_types::CONS => {
                    // For lists, conj adds to the front (like cons)
                    Ok(Expr::StructNew {
                        type_idx: gc_types::CONS,
                        fields: vec![val, coll],
                    })
                }
                _ => Ok(Expr::ProtocolDispatch {
                    obj: Box::new(coll),
                    method_id: method_ids::CONJ,
                    args: vec![val],
                    in_tail_position: self.in_tail_position,
                }),
            };
        }

        // Slow path: runtime protocol dispatch
        Ok(Expr::ProtocolDispatch {
            obj: Box::new(coll),
            method_id: method_ids::CONJ,
            args: vec![val],
            in_tail_position: self.in_tail_position,
        })
    }

    /// Lower (cons val coll) -> StructNew CONS
    fn lower_cons(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 2 {
            return Err(CompileError::Parse(
                "cons requires exactly 2 arguments: value and collection".into(),
            ));
        }
        // Arguments are never in tail position
        let was_tail = self.in_tail_position;
        self.in_tail_position = false;
        let val = self.lower_expr(&args[0])?;
        let coll = self.lower_expr(&args[1])?;
        self.in_tail_position = was_tail;
        // cons creates a new cons cell with type_id, val as first, and coll as rest
        use crate::ir::{gc_types, type_ids};
        Ok(Expr::StructNew {
            type_idx: gc_types::CONS,
            // Field 0: type_id (raw i32), Field 1: first, Field 2: rest
            fields: vec![
                Expr::RawI32(type_ids::CONS),
                val,
                coll,
            ],
        })
    }

    /// Lower (count coll) -> VecCount, MapCount, SetCount, or ProtocolDispatch
    ///
    /// If the collection type is known at compile time, uses the fast path.
    /// Otherwise, falls back to runtime protocol dispatch.
    fn lower_count(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        use crate::ir::{gc_types, method_ids};

        if args.len() != 1 {
            return Err(CompileError::Parse(
                "count requires exactly 1 argument".into(),
            ));
        }

        // Arguments are never in tail position
        let was_tail = self.in_tail_position;
        self.in_tail_position = false;
        let coll = self.lower_expr(&args[0])?;
        self.in_tail_position = was_tail;

        // Fast path: if we know the collection type at compile time
        if let Some(type_id) = self.infer_collection_type(&args[0]) {
            return match type_id {
                t if t == gc_types::PERSISTENT_VECTOR => Ok(Expr::VecCount(Box::new(coll))),
                t if t == gc_types::PERSISTENT_MAP => Ok(Expr::MapCount(Box::new(coll))),
                t if t == gc_types::PERSISTENT_SET => Ok(Expr::SetCount(Box::new(coll))),
                // For lists (CONS), we'd need to walk the list - use protocol dispatch
                _ => Ok(Expr::ProtocolDispatch {
                    obj: Box::new(coll),
                    method_id: method_ids::COUNT,
                    args: vec![],
                    in_tail_position: self.in_tail_position,
                }),
            };
        }

        // Slow path: runtime protocol dispatch
        Ok(Expr::ProtocolDispatch {
            obj: Box::new(coll),
            method_id: method_ids::COUNT,
            args: vec![],
            in_tail_position: self.in_tail_position,
        })
    }

    /// Lower (get coll key) -> MapGet, VecNth, or ProtocolDispatch
    ///
    /// If the collection type is known at compile time, uses the appropriate fast path:
    /// - Maps use MapGet
    /// - Vectors use VecNth (for integer keys)
    /// Otherwise, falls back to runtime protocol dispatch.
    fn lower_get(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        use crate::ir::{gc_types, method_ids};

        if args.len() != 2 {
            return Err(CompileError::Parse(
                "get requires exactly 2 arguments: collection and key".into(),
            ));
        }

        // Arguments are never in tail position
        let was_tail = self.in_tail_position;
        self.in_tail_position = false;
        let coll = self.lower_expr(&args[0])?;
        let key = self.lower_expr(&args[1])?;
        self.in_tail_position = was_tail;

        // Fast path: if we know the collection type at compile time
        if let Some(type_id) = self.infer_collection_type(&args[0]) {
            return match type_id {
                t if t == gc_types::PERSISTENT_MAP => Ok(Expr::MapGet {
                    map: Box::new(coll),
                    key: Box::new(key),
                }),
                t if t == gc_types::PERSISTENT_VECTOR => {
                    // For vectors, get is like nth
                    Ok(Expr::VecNth {
                        vec: Box::new(coll),
                        index: Box::new(key),
                    })
                }
                _ => Ok(Expr::ProtocolDispatch {
                    obj: Box::new(coll),
                    method_id: method_ids::LOOKUP,
                    args: vec![key],
                    in_tail_position: self.in_tail_position,
                }),
            };
        }

        // Slow path: runtime protocol dispatch
        Ok(Expr::ProtocolDispatch {
            obj: Box::new(coll),
            method_id: method_ids::LOOKUP,
            args: vec![key],
            in_tail_position: self.in_tail_position,
        })
    }

    /// Lower (assoc map key val) -> MapAssoc
    fn lower_assoc(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 3 {
            return Err(CompileError::Parse(
                "assoc requires exactly 3 arguments: map, key, and value".into(),
            ));
        }
        let map = self.lower_expr(&args[0])?;
        let key = self.lower_expr(&args[1])?;
        let val = self.lower_expr(&args[2])?;
        Ok(Expr::MapAssoc {
            map: Box::new(map),
            key: Box::new(key),
            val: Box::new(val),
        })
    }

    /// Lower (contains? set key) -> SetContains
    fn lower_contains(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 2 {
            return Err(CompileError::Parse(
                "contains? requires exactly 2 arguments: set and key".into(),
            ));
        }
        // Arguments to contains? are never in tail position
        let was_tail = self.in_tail_position;
        self.in_tail_position = false;
        let set = self.lower_expr(&args[0])?;
        let key = self.lower_expr(&args[1])?;
        self.in_tail_position = was_tail;
        Ok(Expr::SetContains {
            set: Box::new(set),
            key: Box::new(key),
        })
    }

    // ========================================================================
    // Hash Operations
    // ========================================================================

    /// Lower (hash value) -> Hash
    /// Hashes a value using xxHash32 algorithm.
    fn lower_hash(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 1 {
            return Err(CompileError::Parse(
                "hash requires exactly 1 argument".into(),
            ));
        }
        let value = self.lower_expr(&args[0])?;
        Ok(Expr::Hash(Box::new(value)))
    }
}
