//! Lowering from analyzed Suss to IR
//!
//! Converts analyzed s-expressions into the IR representation.

use std::collections::HashMap;

use suss_core::{Sexp, Interner, SymbolId};

use crate::analyze::{AnalyzedModule, AnalyzedFunction, AnalyzedGlobal};
use crate::error::{CompileError, CompileResult};
use crate::ir::{Module, Function, Global, Import, Expr, Type, BinOp, UnOp};

/// Lower analyzed module to IR
pub fn lower(module: &AnalyzedModule, interner: &Interner) -> CompileResult<Module> {
    let mut lowerer = Lowerer::new(interner);
    lowerer.lower_module(module)
}

struct Lowerer<'a> {
    interner: &'a Interner,
    module: Module,
    /// Map from symbol to local index for current function
    locals: HashMap<SymbolId, u32>,
    /// Next local index
    next_local: u32,
    /// Function index map (indices start after imports)
    func_indices: HashMap<SymbolId, u32>,
    /// Import index map: (alias, function_name) -> import index
    import_indices: HashMap<(String, String), u32>,
    /// Number of imports (for calculating function indices)
    num_imports: u32,
}

impl<'a> Lowerer<'a> {
    fn new(interner: &'a Interner) -> Self {
        Self {
            interner,
            module: Module::new(),
            locals: HashMap::new(),
            next_local: 0,
            func_indices: HashMap::new(),
            import_indices: HashMap::new(),
            num_imports: 0,
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
            self.func_indices.insert(func.name, self.num_imports + idx as u32);
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
            name: global.name,
            ty: global.ty.clone(),
            init,
        })
    }

    fn lower_function(&mut self, func: &AnalyzedFunction) -> CompileResult<Function> {
        // Reset locals for new function
        self.locals.clear();
        self.next_local = 0;

        // Add parameters as locals
        let mut params = Vec::new();
        for (sym, ty) in &func.params {
            let idx = self.next_local;
            self.locals.insert(*sym, idx);
            self.next_local += 1;
            params.push((*sym, ty.clone()));
        }

        // Lower body
        let body = self.lower_expr(&func.body)?;

        // Collect local types
        let mut locals = vec![Type::Unknown; self.next_local as usize];
        for (sym, ty) in &params {
            if let Some(&idx) = self.locals.get(sym) {
                locals[idx as usize] = ty.clone();
            }
        }

        Ok(Function {
            name: func.name,
            exported: func.exported,
            export_name: func.export_name.clone(),
            params,
            return_type: func.return_type.clone(),
            locals,
            body,
        })
    }

    fn lower_expr(&mut self, expr: &Sexp) -> CompileResult<Expr> {
        match expr {
            Sexp::Nil => Ok(Expr::Unit),

            Sexp::Bool(b) => Ok(Expr::Bool(*b)),

            Sexp::Number(n) => {
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

            Sexp::String(s) => {
                let idx = self.module.intern_string(s);
                Ok(Expr::String(idx))
            }

            Sexp::Char(c) => Ok(Expr::Int(*c as i64)),

            Sexp::Symbol(sym) => {
                // Check if it's a local
                if let Some(&idx) = self.locals.get(sym) {
                    Ok(Expr::LocalGet(idx))
                } else {
                    let name = self.interner.symbol_name(*sym);
                    Err(CompileError::Undefined(name.to_string()))
                }
            }

            Sexp::List(items) if items.is_empty() => Ok(Expr::Unit),

            Sexp::List(items) => {
                if let Sexp::Symbol(sym) = &items[0] {
                    let name = self.interner.symbol_name(*sym);
                    self.lower_call(name, &items[1..])
                } else {
                    // Function expression call
                    Err(CompileError::Unsupported(
                        "Function expression calls not yet supported".into()
                    ))
                }
            }

            Sexp::Vector(_items) => {
                // Lower to a sequence that builds a vector
                Err(CompileError::Unsupported("Vectors not yet supported".into()))
            }

            _ => Err(CompileError::Unsupported(format!(
                "Expression type not supported: {:?}",
                expr
            ))),
        }
    }

    fn lower_call(&mut self, name: &str, args: &[Sexp]) -> CompileResult<Expr> {
        match name {
            // Arithmetic (default to i32 for WIT compatibility)
            "+" => self.lower_binop_chain(BinOp::Add, args, Type::I32),
            "-" => self.lower_binop_chain(BinOp::Sub, args, Type::I32),
            "*" => self.lower_binop_chain(BinOp::Mul, args, Type::I32),
            "/" => self.lower_binop_chain(BinOp::Div, args, Type::I32),
            "rem" | "mod" => self.lower_binop(BinOp::Rem, args, Type::I32),

            // Comparison
            "=" => self.lower_binop(BinOp::Eq, args, Type::Bool),
            "not=" => self.lower_binop(BinOp::Ne, args, Type::Bool),
            "<" => self.lower_binop(BinOp::Lt, args, Type::Bool),
            "<=" => self.lower_binop(BinOp::Le, args, Type::Bool),
            ">" => self.lower_binop(BinOp::Gt, args, Type::Bool),
            ">=" => self.lower_binop(BinOp::Ge, args, Type::Bool),

            // Logical
            "and" => self.lower_binop_chain(BinOp::And, args, Type::Bool),
            "or" => self.lower_binop_chain(BinOp::Or, args, Type::Bool),
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

            // String
            "str" => self.lower_str(args),

            // Function call
            _ => self.lower_func_call(name, args),
        }
    }

    fn lower_binop(&mut self, op: BinOp, args: &[Sexp], ty: Type) -> CompileResult<Expr> {
        if args.len() != 2 {
            return Err(CompileError::Parse(format!(
                "Binary operator requires exactly 2 arguments, got {}",
                args.len()
            )));
        }
        let left = self.lower_expr(&args[0])?;
        let right = self.lower_expr(&args[1])?;
        Ok(Expr::BinOp {
            op,
            left: Box::new(left),
            right: Box::new(right),
            ty,
        })
    }

    fn lower_binop_chain(&mut self, op: BinOp, args: &[Sexp], ty: Type) -> CompileResult<Expr> {
        if args.is_empty() {
            return Err(CompileError::Parse("Operator requires at least 1 argument".into()));
        }
        if args.len() == 1 {
            return self.lower_expr(&args[0]);
        }

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
        Ok(result)
    }

    fn lower_if(&mut self, args: &[Sexp]) -> CompileResult<Expr> {
        if args.len() < 2 {
            return Err(CompileError::Parse("if requires condition and then branch".into()));
        }

        let cond = self.lower_expr(&args[0])?;
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

    fn lower_do(&mut self, args: &[Sexp]) -> CompileResult<Expr> {
        let exprs: Vec<Expr> = args
            .iter()
            .map(|e| self.lower_expr(e))
            .collect::<CompileResult<_>>()?;
        Ok(Expr::Block(exprs))
    }

    fn lower_let(&mut self, args: &[Sexp]) -> CompileResult<Expr> {
        if args.is_empty() {
            return Err(CompileError::Parse("let requires bindings".into()));
        }

        // Parse bindings vector
        let bindings_vec = match &args[0] {
            Sexp::Vector(items) => items,
            _ => return Err(CompileError::Parse("let bindings must be a vector".into())),
        };

        if bindings_vec.len() % 2 != 0 {
            return Err(CompileError::Parse("let bindings must have even number of forms".into()));
        }

        let mut bindings = Vec::new();
        for chunk in bindings_vec.chunks(2) {
            let name = match &chunk[0] {
                Sexp::Symbol(sym) => *sym,
                _ => return Err(CompileError::Parse("let binding name must be a symbol".into())),
            };

            let value = self.lower_expr(&chunk[1])?;
            let idx = self.next_local;
            self.locals.insert(name, idx);
            self.next_local += 1;
            bindings.push((idx, value));
        }

        // Lower body
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

    fn lower_loop(&mut self, args: &[Sexp]) -> CompileResult<Expr> {
        if args.is_empty() {
            return Err(CompileError::Parse("loop requires bindings".into()));
        }

        // Parse bindings (same as let)
        let bindings_vec = match &args[0] {
            Sexp::Vector(items) => items,
            _ => return Err(CompileError::Parse("loop bindings must be a vector".into())),
        };

        if bindings_vec.len() % 2 != 0 {
            return Err(CompileError::Parse("loop bindings must have even number of forms".into()));
        }

        let mut bindings = Vec::new();
        for chunk in bindings_vec.chunks(2) {
            let name = match &chunk[0] {
                Sexp::Symbol(sym) => *sym,
                _ => return Err(CompileError::Parse("loop binding name must be a symbol".into())),
            };

            let value = self.lower_expr(&chunk[1])?;
            let idx = self.next_local;
            self.locals.insert(name, idx);
            self.next_local += 1;
            bindings.push((idx, value));
        }

        // Lower body
        let body = if args.len() > 1 {
            if args.len() == 2 {
                self.lower_expr(&args[1])?
            } else {
                self.lower_do(&args[1..])?
            }
        } else {
            Expr::Unit
        };

        Ok(Expr::Loop {
            bindings,
            body: Box::new(body),
        })
    }

    fn lower_recur(&mut self, args: &[Sexp]) -> CompileResult<Expr> {
        let values: Vec<Expr> = args
            .iter()
            .map(|e| self.lower_expr(e))
            .collect::<CompileResult<_>>()?;
        Ok(Expr::Recur(values))
    }

    fn lower_str(&mut self, args: &[Sexp]) -> CompileResult<Expr> {
        // First try compile-time concatenation for all-literal strings
        let all_literals = args.iter().all(|arg| matches!(arg, Sexp::String(_)));

        if all_literals && !args.is_empty() {
            // Concatenate at compile time
            let mut result = String::new();
            for arg in args {
                if let Sexp::String(s) = arg {
                    result.push_str(s);
                }
            }
            let idx = self.module.intern_string(&result);
            return Ok(Expr::String(idx));
        }

        // Fall back to runtime concatenation
        let parts: Vec<Expr> = args
            .iter()
            .map(|e| self.lower_expr(e))
            .collect::<CompileResult<_>>()?;
        Ok(Expr::StrConcat(parts))
    }

    fn lower_func_call(&mut self, name: &str, args: &[Sexp]) -> CompileResult<Expr> {
        // Check if it's a qualified import call (e.g., "random/get-random-u64")
        if let Some(slash_pos) = name.find('/') {
            let alias = &name[..slash_pos];
            let func_name = &name[slash_pos + 1..];

            // Look up in import indices
            if let Some(&idx) = self.import_indices.get(&(alias.to_string(), func_name.to_string())) {
                let lowered_args: Vec<Expr> = args
                    .iter()
                    .map(|e| self.lower_expr(e))
                    .collect::<CompileResult<_>>()?;

                return Ok(Expr::Call {
                    func: idx,
                    args: lowered_args,
                });
            } else {
                return Err(CompileError::Undefined(format!(
                    "Import function '{}' not found (alias: '{}', function: '{}')",
                    name, alias, func_name
                )));
            }
        }

        // Check if it's a direct :refer import (no alias)
        if let Some(&idx) = self.import_indices.get(&(String::new(), name.to_string())) {
            let lowered_args: Vec<Expr> = args
                .iter()
                .map(|e| self.lower_expr(e))
                .collect::<CompileResult<_>>()?;

            return Ok(Expr::Call {
                func: idx,
                args: lowered_args,
            });
        }

        // Look up local function by name
        let func_idx = self.func_indices.iter()
            .find(|(sym, _)| self.interner.symbol_name(**sym) == name)
            .map(|(_, idx)| *idx);

        if let Some(idx) = func_idx {
            let lowered_args: Vec<Expr> = args
                .iter()
                .map(|e| self.lower_expr(e))
                .collect::<CompileResult<_>>()?;

            Ok(Expr::Call {
                func: idx,
                args: lowered_args,
            })
        } else {
            Err(CompileError::Undefined(name.to_string()))
        }
    }
}
