//! Lowering from analyzed Suss to IR
//!
//! Converts analyzed s-expressions into the IR representation.

use std::collections::HashMap;

use num_traits::ToPrimitive;
use suss_core::{Edn, Number, Symbol};

use crate::analyze::{
    AnalyzedModule, AnalyzedFunction, AnalyzedGlobal,
    AnalyzedProtocol, AnalyzedExtension, AnalyzedMethodImpl,
    AnalyzedDeftype, DeftypeField,
};
use crate::error::{CompileError, CompileResult};
use crate::ir::{
    Module, Function, Global, Import, Expr, Type, BinOp, UnOp,
    gc_types, type_ids, method_ids, ProtocolDef, DispatchEntry,
    DeftypeDef, DeftypeFieldDef, FieldType,
};

/// Lowering mode - controls whether runtime helpers are included in function indices
#[derive(Clone, Copy, PartialEq)]
pub enum LoweringMode {
    /// Full mode: Runtime helpers and protocol impls are included (for REPL/eval)
    Full,
    /// Component mode: No runtime helpers, user functions start at index 0 (for compile_files)
    Component,
}

/// Lower analyzed module to IR with full runtime helpers
pub fn lower(module: &AnalyzedModule) -> CompileResult<Module> {
    let mut lowerer = Lowerer::new(LoweringMode::Full);
    lowerer.lower_module(module)
}

/// Lower analyzed module to IR for component output (no runtime helpers)
pub fn lower_for_component(module: &AnalyzedModule) -> CompileResult<Module> {
    let mut lowerer = Lowerer::new(LoweringMode::Component);
    lowerer.lower_module(module)
}

/// Represents a closure wrapper function to be generated
struct ClosureWrapper {
    /// Unique name for the wrapper function
    name: String,
    /// Names of captured variables (env[0], env[1], ...)
    captures: Vec<String>,
    /// Parameter names (excluding env)
    params: Vec<String>,
    /// Body expression (original fn body)
    body: Edn,
    /// True for variadic builtin wrappers which don't take an env parameter
    is_variadic: bool,
}

/// Information about a user-defined type field
#[derive(Clone)]
struct UserTypeField {
    name: String,
    field_type: FieldType,
}

/// Information about a user-defined type
#[derive(Clone)]
struct UserTypeInfo {
    gc_type_idx: u32,
    type_id: i32,
    /// Dispatch table slot for protocol dispatch (0-4 for primitives, 5+ for deftypes)
    dispatch_slot: u32,
    fields: Vec<UserTypeField>,
}

struct Lowerer {
    module: Module,
    /// Lowering mode - controls function index calculation
    mode: LoweringMode,
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
    /// Whether current expression is in tail position within a loop (for recur validation)
    in_loop_tail: bool,
    /// Current loop binding local indices (for recur)
    loop_binding_locals: Vec<u32>,
    /// Counter for generating unique closure wrapper names
    closure_counter: u32,
    /// Closure wrappers to be generated after main function lowering
    pending_closures: Vec<ClosureWrapper>,
    /// Total number of analyzed (module-level) functions
    /// Used to calculate closure wrapper indices correctly
    num_analyzed_funcs: u32,
    /// Number of deftype constructor functions
    /// Used to calculate closure wrapper indices correctly
    num_deftype_constructors: u32,
    /// Number of protocol method implementations from extend-type
    /// Used to calculate closure wrapper indices correctly
    num_extension_funcs: u32,
    /// Cached indices for built-in wrapper functions: builtin_name -> func_idx
    builtin_wrappers: HashMap<String, u32>,
    /// User-defined protocol method IDs: "ProtocolName/method_name/arity" -> method_id
    user_method_ids: HashMap<String, u32>,
    /// Reverse lookup: method_name -> set of (protocol_name, arity) pairs
    /// Used to identify protocol method calls at call sites
    method_protocols: HashMap<String, Vec<(String, usize)>>,
    /// Next user-defined method ID (starts at method_ids::USER_START)
    next_user_method_id: u32,
    /// Protocol method return types: method_name -> type_hint (e.g., "i32")
    /// Populated from ^type hints in defprotocol definitions
    method_return_types: HashMap<String, String>,
    /// User-defined types: name -> type info
    user_types: HashMap<String, UserTypeInfo>,
    /// Next GC type index for user types (starts at NUM_GC_TYPES)
    next_user_gc_type: u32,
    /// Next type ID for user types (starts at USER_TYPE_BASE)
    next_user_type_id: i32,
    /// Next dispatch table slot for deftypes (starts at 5, after primitive slots 0-4)
    next_dispatch_slot: u32,
    /// Current self type when lowering protocol method implementations
    /// Used to resolve field access to the correct type
    current_self_type: Option<String>,
}

impl Lowerer {
    fn new(mode: LoweringMode) -> Self {
        // Pre-populate method_protocols with built-in protocol methods
        // Format: method_name -> [(protocol_name, arity), ...]
        let mut method_protocols = HashMap::new();

        // Built-in protocol methods with their arities (including 'this' param)
        // ICounted: -count [coll] -> arity 1
        method_protocols.insert("-count".to_string(), vec![("ICounted".to_string(), 1)]);
        // IIndexed: -nth [coll n] or [coll n not-found] -> arities 2, 3
        method_protocols.insert("-nth".to_string(), vec![
            ("IIndexed".to_string(), 2),
            ("IIndexed".to_string(), 3),
        ]);
        // ISeq: -first [seq], -rest [seq] -> arity 1
        method_protocols.insert("-first".to_string(), vec![("ISeq".to_string(), 1)]);
        method_protocols.insert("-rest".to_string(), vec![("ISeq".to_string(), 1)]);
        // ISeqable: -seq [coll] -> arity 1
        method_protocols.insert("-seq".to_string(), vec![("ISeqable".to_string(), 1)]);
        // ILookup: -lookup [coll key] or [coll key not-found] -> arities 2, 3
        method_protocols.insert("-lookup".to_string(), vec![
            ("ILookup".to_string(), 2),
            ("ILookup".to_string(), 3),
        ]);
        // IAssociative: -assoc [coll key val] -> arity 3
        method_protocols.insert("-assoc".to_string(), vec![("IAssociative".to_string(), 3)]);
        // ICollection: -conj [coll val] -> arity 2
        method_protocols.insert("-conj".to_string(), vec![("ICollection".to_string(), 2)]);
        // IHash: -hash [o] -> arity 1
        method_protocols.insert("-hash".to_string(), vec![("IHash".to_string(), 1)]);
        // IEquiv: -equiv [o other] -> arity 2
        method_protocols.insert("-equiv".to_string(), vec![("IEquiv".to_string(), 2)]);

        Self {
            module: Module::new(),
            mode,
            locals: HashMap::new(),
            local_types: HashMap::new(),
            next_local: 0,
            func_indices: HashMap::new(),
            import_indices: HashMap::new(),
            num_imports: 0,
            in_tail_position: false,
            in_loop_tail: false,
            loop_binding_locals: Vec::new(),
            closure_counter: 0,
            pending_closures: Vec::new(),
            num_analyzed_funcs: 0,
            num_deftype_constructors: 0,
            num_extension_funcs: 0,
            builtin_wrappers: HashMap::new(),
            user_method_ids: HashMap::new(),
            method_protocols,
            next_user_method_id: method_ids::USER_START,
            method_return_types: HashMap::new(),
            user_types: HashMap::new(),
            next_user_gc_type: gc_types::NUM_GC_TYPES,
            next_user_type_id: type_ids::USER_TYPE_BASE,
            next_dispatch_slot: 5, // slots 0-4 reserved for primitives
            current_self_type: None,
        }
    }

    // ========================================================================
    // Tail Position Context Helpers
    // ========================================================================

    /// Evaluate an expression with tail position tracking temporarily disabled.
    /// Restores both in_tail_position and in_loop_tail after the closure runs.
    /// Returns the saved tail position for use in tail-aware IR nodes.
    fn with_args_context<F, R>(&mut self, f: F) -> (R, bool)
    where
        F: FnOnce(&mut Self) -> R,
    {
        let was_tail = self.in_tail_position;
        let was_loop_tail = self.in_loop_tail;
        self.in_tail_position = false;
        self.in_loop_tail = false;

        let result = f(self);

        self.in_tail_position = was_tail;
        self.in_loop_tail = was_loop_tail;
        (result, was_tail)
    }

    /// Simpler version when we don't need the saved tail position.
    fn with_tail_disabled<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut Self) -> R,
    {
        self.with_args_context(f).0
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
        self.num_analyzed_funcs = analyzed.functions.len() as u32;

        // Lower protocol definitions first - populates method_return_types
        // which is needed by lower_deftypes and lower_extensions for return type hints
        self.lower_protocols(&analyzed.protocols)?;

        // Lower deftypes - assigns GC type indices and type IDs
        // This creates constructor functions for deftypes (skipping HAMT nodes 5-7)
        self.lower_deftypes(&analyzed.deftypes)?;
        // Count only deftypes that generate constructors (skip HAMT nodes 5-7)
        self.num_deftype_constructors = analyzed
            .deftypes
            .iter()
            .filter(|dt| {
                use crate::ir::gc_types;
                let gc_type_idx = dt.reserved_type_id.unwrap_or(gc_types::NUM_GC_TYPES);
                gc_type_idx < gc_types::BITMAP_INDEXED_NODE || gc_type_idx > gc_types::HASH_COLLISION_NODE
            })
            .count() as u32;
        let num_deftype_constructors = self.num_deftype_constructors;

        // Pre-count extension methods (from extend-type) so closure indices are correct
        // Each extension can have multiple protocol implementations, each with multiple methods
        self.num_extension_funcs = analyzed
            .extensions
            .iter()
            .flat_map(|ext| ext.implementations.iter())
            .map(|impl_| impl_.methods.len() as u32)
            .sum();

        // Build function index map
        //
        // Function indices in the generated WASM are laid out as:
        //   [0..N)                          - WASI/WIT imports
        //   [N..N+H)                        - Runtime helpers (hash_string, get_type_id)
        //   [N+H..N+H+C)                    - Deftype constructors
        //   [N+H+C..N+H+C+F)                - User functions (core.suss + user code)
        //
        // Where: N = num_imports, H = NUM_RUNTIME_HELPERS (2), C = num_deftype_constructors
        //
        // IMPORTANT: Both REPL and Component modes use the same offset because runtime
        // helpers are emitted in both modes. This ensures function calls work correctly
        // in both contexts.
        use crate::ir::gc_types;
        let func_offset = gc_types::USER_FUNC_OFFSET;
        for (idx, func) in analyzed.functions.iter().enumerate() {
            let func_idx = self.num_imports + func_offset + num_deftype_constructors + idx as u32;
            self.func_indices.insert(func.name.clone(), func_idx);
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

        // Lower extend-type declarations (generates dispatch table entries)
        // Note: lower_protocols was already called earlier to populate method_return_types
        self.lower_extensions(&analyzed.extensions)?;

        // Generate closure wrapper functions
        // We need to process these iteratively since lowering a closure body
        // might create more closures
        while !self.pending_closures.is_empty() {
            let closures = std::mem::take(&mut self.pending_closures);
            for closure in closures {
                let wrapper = self.lower_closure_wrapper(&closure)?;
                self.module.functions.push(wrapper);
            }
        }

        Ok(std::mem::take(&mut self.module))
    }

    /// Generate a wrapper function for a closure
    /// Regular closure signature: (env: eqref, params...) -> eqref
    /// Variadic closure signature: (params...) -> eqref (no env)
    fn lower_closure_wrapper(&mut self, closure: &ClosureWrapper) -> CompileResult<Function> {
        use crate::ir::gc_types;

        // Reset locals for new function
        self.locals.clear();
        self.local_types.clear();
        self.next_local = 0;

        let mut params = Vec::new();
        let env_idx: Option<u32>;

        // All closures (including variadic) now have env as first parameter
        // to match CLOSURE_FN_* function types. Variadic builtins just ignore env.
        env_idx = Some(self.next_local);
        self.next_local += 1;
        self.local_types.insert(env_idx.unwrap(), Type::GcRef);
        params.push(("$env".to_string(), Type::GcRef));

        // Add regular parameters
        for param_name in &closure.params {
            let idx = self.next_local;
            self.locals.insert(param_name.clone(), (idx, Type::GcRef));
            self.local_types.insert(idx, Type::GcRef);
            self.next_local += 1;
            params.push((param_name.clone(), Type::GcRef));
        }

        // Add captured variables as synthetic locals that read from env
        // (only for non-variadic closures with env)
        let mut capture_bindings = Vec::new();
        if let Some(env) = env_idx {
            for (cap_idx, cap_name) in closure.captures.iter().enumerate() {
                let local_idx = self.next_local;
                self.locals.insert(cap_name.clone(), (local_idx, Type::GcRef));
                self.local_types.insert(local_idx, Type::GcRef);
                self.next_local += 1;

                // env[cap_idx] - ArrayGet from env
                // Note: index must be a boxed integer (Int), not RawI32,
                // because codegen for ArrayGet expects an i31ref to unbox
                let env_get = Expr::ArrayGet {
                    type_idx: gc_types::TRIE_NODE,
                    array: Box::new(Expr::LocalGet {
                        local: env,
                        ty: Type::GcRef,
                    }),
                    index: Box::new(Expr::Int(cap_idx as i64)),
                };
                capture_bindings.push((local_idx, env_get));
            }
        }

        // Lower the body with captures in scope
        self.in_tail_position = true;
        let body_expr = self.lower_expr(&closure.body)?;
        self.in_tail_position = false;

        // Wrap body with capture bindings if any
        let final_body = if capture_bindings.is_empty() {
            body_expr
        } else {
            Expr::Let {
                bindings: capture_bindings,
                body: Box::new(body_expr),
            }
        };

        // Collect local types
        let mut locals = vec![Type::Unknown; self.next_local as usize];
        for (&idx, ty) in &self.local_types {
            locals[idx as usize] = ty.clone();
        }

        Ok(Function {
            name: closure.name.clone(),
            exported: false,
            export_name: None,
            params,
            return_type: Type::GcRef,
            has_explicit_return_type: false,
            locals,
            body: final_body,
        })
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

        // Determine return type and whether it was explicitly hinted
        let (return_type, has_explicit_return_type) = if let Some(ref hint) = func.return_type_hint {
            let ty = match hint.as_str() {
                "i32" => Type::I32,
                "i64" => Type::I64,
                "f64" => Type::F64,
                "eqref" => Type::GcRef,
                _ => func.return_type.clone(), // Unknown hint, use inferred
            };
            (ty, true)
        } else {
            (func.return_type.clone(), false)
        };

        Ok(Function {
            name: func.name.clone(),
            exported: func.exported,
            export_name: func.export_name.clone(),
            params,
            return_type,
            has_explicit_return_type,
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
                } else if let Some(arity) = self.builtin_arity(&sym.name) {
                    // It's a built-in in value position - wrap it as a closure
                    self.lower_builtin_as_closure(&sym.name, arity)
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
                    // Expression in call position - treat as closure call
                    // Examples: ((fn [x] x) 5), ((if cond + -) a b)
                    let (closure, args) = self.with_tail_disabled(|l| -> CompileResult<_> {
                        let closure = l.lower_expr(&items[0])?;
                        let args: Vec<Expr> = items[1..]
                            .iter()
                            .map(|e| l.lower_expr(e))
                            .collect::<CompileResult<_>>()?;
                        Ok((closure, args))
                    })?;
                    Ok(Expr::ClosureCall {
                        closure: Box::new(closure),
                        args,
                        in_tail_position: self.in_tail_position,
                    })
                }
            }

            Edn::Vector(items) => {
                if items.is_empty() {
                    // Empty vector: use VecNew for minimal codegen
                    Ok(Expr::VecNew(vec![]))
                } else {
                    // Desugar [1 2 3] -> (conj (conj (conj [] 1) 2) 3)
                    // Start with empty vector
                    let mut result = Edn::Vector(vec![]);
                    // Wrap each element with (conj ... elem)
                    for item in items {
                        let conj_call = Edn::List(vec![
                            Edn::Symbol(Symbol::new("conj")),
                            result,
                            item.clone(),
                        ]);
                        result = conj_call;
                    }
                    self.lower_expr(&result)
                }
            }

            Edn::Map(pairs) => {
                if pairs.is_empty() {
                    // Empty map: use MapNew for minimal codegen
                    Ok(Expr::MapNew(vec![]))
                } else {
                    // Desugar {1 2 3 4} -> (assoc (assoc {} 1 2) 3 4)
                    // Start with empty map
                    let mut result = Edn::Map(vec![]);
                    // Wrap each pair with (assoc ... k v)
                    for (k, v) in pairs {
                        let assoc_call = Edn::List(vec![
                            Edn::Symbol(Symbol::new("assoc")),
                            result,
                            k.clone(),
                            v.clone(),
                        ]);
                        result = assoc_call;
                    }
                    self.lower_expr(&result)
                }
            }

            Edn::Set(items) => {
                if items.is_empty() {
                    // Empty set: use SetNew for minimal codegen
                    Ok(Expr::SetNew(vec![]))
                } else {
                    // Desugar #{1 2 3} -> (conj (conj (conj #{} 1) 2) 3)
                    // Start with empty set
                    let mut result = Edn::Set(vec![]);
                    // Wrap each element with (conj ... elem)
                    for item in items {
                        let conj_call = Edn::List(vec![
                            Edn::Symbol(Symbol::new("conj")),
                            result,
                            item.clone(),
                        ]);
                        result = conj_call;
                    }
                    self.lower_expr(&result)
                }
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
                // Division always produces F64 (Clojure semantics: / returns ratio/float)
                self.lower_binop_chain(BinOp::Div, args, Type::F64)
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

            // Logical
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

            // nil? - check if value is nil (compares with NIL_SENTINEL)
            "nil?" => {
                if args.len() != 1 {
                    return Err(CompileError::Parse("nil? requires exactly 1 argument".into()));
                }
                let operand = self.lower_expr(&args[0])?;
                // nil? returns true if operand equals nil sentinel (i31ref(0))
                Ok(Expr::NilCheck(Box::new(operand)))
            }

            // Control flow
            "if" => self.lower_if(args),
            "do" => self.lower_do(args),
            "let" => self.lower_let(args),
            "loop" => self.lower_loop(args),
            "recur" => self.lower_recur(args),

            // First-class functions
            "fn" => self.lower_fn(args),
            "apply" => self.lower_apply(args),

            // String
            "str" => self.lower_str(args),

            // Collection operations
            "nth" => self.lower_nth(args),
            "first" => self.lower_first(args),
            "rest" => self.lower_rest(args),
            "conj" => self.lower_conj(args),
            // NOTE: "cons" is now a regular function in core.suss, not a compiler builtin
            "count" => self.lower_count(args),
            "get" => self.lower_get(args),
            "assoc" => self.lower_assoc(args),
            "dissoc" => self.lower_dissoc(args),
            "contains?" => self.lower_contains(args),
            "disj" => self.lower_disj(args),

            // Hashing
            "hash" => self.lower_hash(args),

            // Bit manipulation
            "bit-and" => self.lower_binop(BinOp::BitAnd, args, Type::I32),
            "bit-or" => self.lower_binop(BinOp::BitOr, args, Type::I32),
            "bit-xor" => self.lower_binop(BinOp::BitXor, args, Type::I32),
            "bit-shift-left" => self.lower_binop(BinOp::Shl, args, Type::I32),
            "bit-shift-right" => self.lower_binop(BinOp::ShrS, args, Type::I32),
            "unsigned-bit-shift-right" => self.lower_binop(BinOp::ShrU, args, Type::I32),
            "bit-count" => self.lower_bit_count(args),

            // Array operations (ClojureScript naming: aget, aset, alength, aclone)
            "aget" => self.lower_aget(args),
            "aset" => self.lower_aset(args),
            "alength" => self.lower_alength(args),
            "aclone" => self.lower_aclone(args),
            "acopy" => self.lower_acopy(args),
            "make-array" => self.lower_make_array(args),

            // Type checking
            "instance?" => self.lower_instance_check(args),

            // Field access (.-field syntax)
            _ if name.starts_with(".-") => self.lower_field_access(name, args),

            // Protocol method calls (dispatched via protocol table)
            _ if self.is_protocol_method(name) => {
                self.lower_protocol_method_call(name, args)
            }

            // Function call
            _ => self.lower_func_call(name, args),
        }
    }

    /// Lower field access: (.-field obj)
    /// Field names are mapped based on known struct types.
    fn lower_field_access(&mut self, name: &str, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 1 {
            return Err(CompileError::Parse(format!(
                "Field access requires exactly 1 argument, got {}",
                args.len()
            )));
        }

        let field_name = &name[2..]; // Strip ".-" prefix

        // The object expression is not in tail position - we still need to access its field
        let obj = self.with_tail_disabled(|l| l.lower_expr(&args[0]))?;

        // Check user-defined types first
        if let Some((type_idx, field_idx, field_type)) = self.lookup_user_field(field_name) {
            // For i32 fields, we use StructGetI32 which codegen will handle specially
            // (encode as tagged i31ref). For other types, use regular StructGet.
            let expr = match field_type {
                FieldType::I32 => {
                    // StructGetI32 signals codegen to encode the i32 as tagged i31ref
                    Expr::StructGetI32 {
                        type_idx,
                        field_idx,
                        value: Box::new(obj),
                    }
                }
                FieldType::I64 => {
                    // Wrap i64 as INT64 struct
                    let raw_access = Expr::StructGet {
                        type_idx,
                        field_idx,
                        value: Box::new(obj),
                    };
                    Expr::StructNew {
                        type_idx: gc_types::INT64,
                        fields: vec![
                            Expr::RawI32(type_ids::INT64), // type_id
                            raw_access, // the i64 value
                        ],
                    }
                }
                FieldType::F64 => {
                    // Wrap f64 as FLOAT64 struct
                    let raw_access = Expr::StructGet {
                        type_idx,
                        field_idx,
                        value: Box::new(obj),
                    };
                    Expr::StructNew {
                        type_idx: gc_types::FLOAT64,
                        fields: vec![
                            Expr::RawI32(type_ids::FLOAT64), // type_id
                            raw_access, // the f64 value
                        ],
                    }
                }
                FieldType::GcRef => {
                    Expr::StructGet {
                        type_idx,
                        field_idx,
                        value: Box::new(obj),
                    }
                }
            };

            return Ok(expr);
        }

        // Map field names to (type_idx, field_idx) pairs for built-in types
        // PersistentVector: { type_id: 0, cnt: 1, shift: 2, root: 3, tail: 4 }
        // PersistentMap: { type_id: 0, cnt: 1, root: 2 }
        // PersistentSet: { type_id: 0, cnt: 1, root: 2, _marker: 3 }
        // Cons: { type_id: 0, first: 1, rest: 2 }

        let (type_idx, field_idx) = match field_name {
            // Vector fields
            "cnt" => (gc_types::PERSISTENT_VECTOR, 1),
            "shift" => (gc_types::PERSISTENT_VECTOR, 2),
            "root" => (gc_types::PERSISTENT_VECTOR, 3),
            "tail" => (gc_types::PERSISTENT_VECTOR, 4),
            // Cons fields (type_id is at 0)
            "first" => (gc_types::CONS, 1),
            "rest" => (gc_types::CONS, 2),
            // BitmapIndexedNode fields: { type_id: 0, bitmap: 1, arr: 2 }
            "bitmap" => (gc_types::BITMAP_INDEXED_NODE, 1),
            "bin-arr" => (gc_types::BITMAP_INDEXED_NODE, 2),
            // ArrayNode fields: { type_id: 0, cnt: 1, arr: 2 }
            "an-cnt" => (gc_types::ARRAY_NODE, 1),
            "an-arr" => (gc_types::ARRAY_NODE, 2),
            // HashCollisionNode fields: { type_id: 0, hash: 1, cnt: 2, arr: 3 }
            "hash" => (gc_types::HASH_COLLISION_NODE, 1),
            "hcn-cnt" => (gc_types::HASH_COLLISION_NODE, 2),
            "hcn-arr" => (gc_types::HASH_COLLISION_NODE, 3),
            // PersistentMap fields: { type_id: 0, cnt: 1, root: 2 }
            "map-cnt" => (gc_types::PERSISTENT_MAP, 1),
            "map-root" => (gc_types::PERSISTENT_MAP, 2),
            // PersistentSet fields: { type_id: 0, cnt: 1, root: 2, _marker: 3 }
            "set-cnt" => (gc_types::PERSISTENT_SET, 1),
            "set-root" => (gc_types::PERSISTENT_SET, 2),
            // Add more as needed
            _ => return Err(CompileError::Undefined(format!("Unknown field: {}", field_name))),
        };

        Ok(Expr::StructGet {
            type_idx,
            field_idx,
            value: Box::new(obj),
        })
    }

    /// Look up a field in user-defined types.
    /// Returns (gc_type_idx, field_idx, field_type) if found.
    /// Field index is offset by 1 to account for type_id at field 0.
    ///
    /// Collection types (Cons, PersistentVector, etc.) are now regular user types
    /// defined in core.suss, so all deftypes are handled uniformly here.
    ///
    /// If `current_self_type` is set (during protocol method lowering), prioritize
    /// looking up fields in that type to handle fields with the same name in
    /// different types (e.g., "root" exists in both PersistentVector and PersistentMap).
    fn lookup_user_field(&self, field_name: &str) -> Option<(u32, u32, FieldType)> {
        // If we're in a protocol method implementation, check the self type first
        if let Some(ref self_type) = self.current_self_type {
            if let Some(info) = self.user_types.get(self_type.as_str()) {
                for (idx, field) in info.fields.iter().enumerate() {
                    if field.name == field_name {
                        // Field 0 is type_id, so user fields start at index 1
                        return Some((info.gc_type_idx, (idx + 1) as u32, field.field_type));
                    }
                }
            }
        }

        // Fall back to searching all user types
        for info in self.user_types.values() {
            for (idx, field) in info.fields.iter().enumerate() {
                if field.name == field_name {
                    // Field 0 is type_id, so user fields start at index 1
                    return Some((info.gc_type_idx, (idx + 1) as u32, field.field_type));
                }
            }
        }
        None
    }

    /// Lower instance check: (instance? TypeName obj)
    fn lower_instance_check(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 2 {
            return Err(CompileError::Parse(format!(
                "instance? requires exactly 2 arguments, got {}",
                args.len()
            )));
        }

        let type_name = match &args[0] {
            Edn::Symbol(sym) => &sym.name,
            _ => return Err(CompileError::Parse("instance? first arg must be a type name symbol".into())),
        };

        // The object expression is not in tail position - we still need to test it
        let obj = self.with_tail_disabled(|l| l.lower_expr(&args[1]))?;

        // Check user-defined types first (includes collection types from core.suss)
        if let Some(info) = self.user_types.get(type_name.as_str()) {
            return Ok(Expr::RefTest {
                type_idx: info.gc_type_idx,
                value: Box::new(obj),
            });
        }

        // Fallback for primitive types only
        let type_idx = match type_name.as_str() {
            "String" => gc_types::STRING,
            _ => return Err(CompileError::Undefined(format!("Unknown type: {}", type_name))),
        };

        Ok(Expr::RefTest {
            type_idx,
            value: Box::new(obj),
        })
    }

    fn lower_binop(&mut self, op: BinOp, args: &[Edn], ty: Type) -> CompileResult<Expr> {
        if args.len() != 2 {
            return Err(CompileError::Parse(format!(
                "Binary operator requires exactly 2 arguments, got {}",
                args.len()
            )));
        }

        // Arguments to binary operations are never in tail position
        let (left, right) = self.with_tail_disabled(|l| -> CompileResult<_> {
            Ok((l.lower_expr(&args[0])?, l.lower_expr(&args[1])?))
        })?;
        Ok(Expr::BinOp {
            op,
            left: Box::new(left),
            right: Box::new(right),
            ty,
        })
    }

    fn lower_binop_chain(&mut self, op: BinOp, args: &[Edn], ty: Type) -> CompileResult<Expr> {
        // Handle zero-arg cases: (+) → 0, (*) → 1
        if args.is_empty() {
            return match op {
                BinOp::Add => Ok(Expr::Int(0)),
                BinOp::Mul => Ok(Expr::Int(1)),
                _ => Err(CompileError::Parse(
                    format!("{:?} requires at least 1 argument", op),
                )),
            };
        }

        // Handle single-arg cases
        if args.len() == 1 {
            // Arguments to binary operations are never in tail position
            let x = self.with_tail_disabled(|l| l.lower_expr(&args[0]))?;

            return match op {
                // (- x) → negate: 0 - x
                BinOp::Sub => Ok(Expr::BinOp {
                    op: BinOp::Sub,
                    left: Box::new(Expr::Int(0)),
                    right: Box::new(x),
                    ty,
                }),
                // (/ x) → reciprocal: 1.0 / x
                BinOp::Div => {
                    let x_float = Self::coerce_to_float(x);
                    Ok(Expr::BinOp {
                        op: BinOp::Div,
                        left: Box::new(Expr::Float(1.0)),
                        right: Box::new(x_float),
                        ty: Type::F64,
                    })
                }
                // (+ x) and (* x) just return x
                _ => Ok(x),
            };
        }

        // Arguments to binary operations are never in tail position
        self.with_tail_disabled(|l| {
            let mut result = l.lower_expr(&args[0])?;
            // For division, coerce operands to float
            if op == BinOp::Div {
                result = Self::coerce_to_float(result);
            }

            for arg in &args[1..] {
                let mut right = l.lower_expr(arg)?;
                // For division, coerce operands to float
                if op == BinOp::Div {
                    right = Self::coerce_to_float(right);
                }
                result = Expr::BinOp {
                    op,
                    left: Box::new(result),
                    right: Box::new(right),
                    ty: ty.clone(),
                };
            }

            Ok(result)
        })
    }

    /// Coerce an expression to float. If it's an integer literal, convert it.
    /// For other expressions, wrap in a ToFloat conversion.
    fn coerce_to_float(expr: Expr) -> Expr {
        match expr {
            Expr::Int(i) => Expr::Float(i as f64),
            other => Expr::ToFloat(Box::new(other)),
        }
    }

    fn lower_if(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() < 2 {
            return Err(CompileError::Parse("if requires condition and then branch".into()));
        }

        // Condition is never in tail position (neither TCO nor loop tail)
        let cond = self.with_tail_disabled(|l| l.lower_expr(&args[0]))?;

        // Both branches inherit parent's tail positions (restored by with_tail_disabled)
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

        let mut exprs = Vec::with_capacity(args.len());

        // All but the last expression are NOT in tail position (neither TCO nor loop tail)
        for arg in &args[..args.len() - 1] {
            exprs.push(self.with_tail_disabled(|l| l.lower_expr(arg))?);
        }

        // Last expression inherits tail positions (unchanged)
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

        // Bindings are NOT in tail position (neither TCO nor loop tail)
        let mut bindings = Vec::new();
        for chunk in bindings_vec.chunks(2) {
            let name = match &chunk[0] {
                Edn::Symbol(sym) => sym.name.clone(),
                _ => return Err(CompileError::Parse("let binding name must be a symbol".into())),
            };

            let value = self.with_tail_disabled(|l| l.lower_expr(&chunk[1]))?;
            let value_ty = value.expr_type();
            let idx = self.next_local;
            self.locals.insert(name, (idx, value_ty.clone()));
            self.local_types.insert(idx, value_ty);
            self.next_local += 1;
            bindings.push((idx, value));
        }

        // Body inherits tail positions (unchanged)
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

    /// Lower anonymous function to closure
    /// (fn [params] body) → ClosureNew { func_idx, arity, captures }
    fn lower_fn(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.is_empty() {
            return Err(CompileError::Parse("fn requires params vector".into()));
        }

        // Parse params vector
        let params_vec = match &args[0] {
            Edn::Vector(items) => items,
            _ => return Err(CompileError::Parse("fn params must be a vector".into())),
        };

        let mut params = Vec::new();
        for param in params_vec {
            match param {
                Edn::Symbol(sym) => params.push(sym.name.clone()),
                _ => return Err(CompileError::Parse("fn param must be a symbol".into())),
            }
        }

        // Body is everything after params, wrapped in do if multiple
        let body = if args.len() == 2 {
            args[1].clone()
        } else if args.len() > 2 {
            let do_sym = Edn::Symbol(suss_core::Symbol::new("do"));
            let mut body_items = vec![do_sym];
            body_items.extend(args[1..].iter().cloned());
            Edn::List(body_items)
        } else {
            Edn::Nil
        };

        // Collect free variables in body that aren't params
        let free_vars = self.collect_free_vars(&body, &params);

        // Generate unique wrapper function name
        let wrapper_name = format!("$closure_{}", self.closure_counter);
        self.closure_counter += 1;

        // Calculate wrapper function index in IR
        // Closures are added after deftype constructors, analyzed functions, and extension methods
        let num_pending = self.pending_closures.len() as u32;
        let wrapper_idx = self.num_deftype_constructors + self.num_analyzed_funcs + self.num_extension_funcs + num_pending;

        // Register wrapper function name for the index
        self.func_indices.insert(wrapper_name.clone(), wrapper_idx);

        // Store closure wrapper for later generation
        self.pending_closures.push(ClosureWrapper {
            name: wrapper_name,
            captures: free_vars.clone(),
            params,
            body,
            is_variadic: false,
        });

        // Generate capture expressions (LocalGet for each captured variable)
        let captures: Vec<Expr> = free_vars
            .iter()
            .map(|name| {
                if let Some(&(idx, ref ty)) = self.locals.get(name) {
                    Ok(Expr::LocalGet {
                        local: idx,
                        ty: ty.clone(),
                    })
                } else {
                    Err(CompileError::Undefined(name.clone()))
                }
            })
            .collect::<CompileResult<Vec<_>>>()?;

        let arity = self.pending_closures.last().unwrap().params.len() as u32;

        Ok(Expr::ClosureNew {
            func_idx: wrapper_idx,
            arity,
            captures,
        })
    }

    /// Lower (apply func args-vec) to Apply expression.
    ///
    /// Supports the simple two-argument form:
    /// (apply + [1 2 3]) -> calls + with three arguments from the vector
    fn lower_apply(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 2 {
            return Err(CompileError::Parse(
                "apply requires exactly 2 arguments: function and arg collection".into(),
            ));
        }

        // Arguments are never in tail position
        let (func, arg_coll) = self.with_tail_disabled(|l| -> CompileResult<_> {
            Ok((l.lower_expr(&args[0])?, l.lower_expr(&args[1])?))
        })?;

        Ok(Expr::Apply {
            func: Box::new(func),
            args: Box::new(arg_coll),
        })
    }

    /// Collect free variables in an expression that aren't in the given bound set
    fn collect_free_vars(&self, expr: &Edn, bound: &[String]) -> Vec<String> {
        let mut free = Vec::new();
        self.collect_free_vars_inner(expr, bound, &mut free);
        // Remove duplicates while preserving order
        let mut seen = std::collections::HashSet::new();
        free.retain(|x| seen.insert(x.clone()));
        free
    }

    fn collect_free_vars_inner(&self, expr: &Edn, bound: &[String], free: &mut Vec<String>) {
        match expr {
            Edn::Symbol(sym) => {
                let name = &sym.name;
                // Check if it's a free variable:
                // - Not in bound list
                // - Is a local in current scope (not a global or function)
                if !bound.contains(name) && self.locals.contains_key(name) {
                    free.push(name.clone());
                }
            }
            Edn::List(items) | Edn::Vector(items) => {
                if let Some((first, rest)) = items.split_first() {
                    // Check for binding forms that introduce new bindings
                    if let Edn::Symbol(sym) = first {
                        match sym.name.as_str() {
                            "let" | "loop" => {
                                // (let [x 1 y 2] body) - bindings introduce new scope
                                if let Some(Edn::Vector(bindings)) = rest.first() {
                                    let mut new_bound: Vec<String> = bound.to_vec();
                                    for chunk in bindings.chunks(2) {
                                        if let Some(Edn::Symbol(bsym)) = chunk.first() {
                                            new_bound.push(bsym.name.clone());
                                        }
                                        // Also check the value expression with current bindings
                                        if let Some(val) = chunk.get(1) {
                                            self.collect_free_vars_inner(val, &new_bound, free);
                                        }
                                    }
                                    // Check body with extended bindings
                                    for body_expr in rest.iter().skip(1) {
                                        self.collect_free_vars_inner(body_expr, &new_bound, free);
                                    }
                                    return;
                                }
                            }
                            "fn" => {
                                // Nested fn - its params are bound in its body
                                if let Some(Edn::Vector(params)) = rest.first() {
                                    let mut new_bound: Vec<String> = bound.to_vec();
                                    for p in params {
                                        if let Edn::Symbol(psym) = p {
                                            new_bound.push(psym.name.clone());
                                        }
                                    }
                                    for body_expr in rest.iter().skip(1) {
                                        self.collect_free_vars_inner(body_expr, &new_bound, free);
                                    }
                                    return;
                                }
                            }
                            _ => {}
                        }
                    }
                }
                // Default: recurse into all items
                for item in items {
                    self.collect_free_vars_inner(item, bound, free);
                }
            }
            Edn::Map(entries) => {
                for (k, v) in entries {
                    self.collect_free_vars_inner(k, bound, free);
                    self.collect_free_vars_inner(v, bound, free);
                }
            }
            Edn::Set(items) => {
                for item in items {
                    self.collect_free_vars_inner(item, bound, free);
                }
            }
            // Literals don't contain free variables
            _ => {}
        }
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

        // Save and set loop tail position - body IS in loop tail position (recur is valid there)
        let was_loop_tail = self.in_loop_tail;
        self.in_loop_tail = true;

        // Lower body (not in TCO tail position, but in loop tail position for recur)
        let body = if args.len() > 1 {
            if args.len() == 2 {
                self.lower_expr(&args[1])?
            } else {
                self.lower_do(&args[1..])?
            }
        } else {
            Expr::Unit
        };

        // Restore old loop bindings and tail positions
        self.loop_binding_locals = old_loop_bindings;
        self.in_tail_position = was_tail;
        self.in_loop_tail = was_loop_tail;

        Ok(Expr::Loop {
            bindings,
            body: Box::new(body),
        })
    }

    fn lower_recur(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        // Check that we're inside a loop
        if self.loop_binding_locals.is_empty() {
            return Err(CompileError::Semantic(
                "recur can only appear inside a loop".into()
            ));
        }

        // Check that we're in tail position of the loop (ClojureScript semantics)
        if !self.in_loop_tail {
            return Err(CompileError::Semantic(
                "recur can only appear in tail position of loop".into()
            ));
        }

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
        let n = self.with_tail_disabled(|l| l.lower_expr(&args[0]))?;
        let ty = n.expr_type();
        // GcRef means boxed value - treat as I32 for arithmetic
        let result_ty = match &ty {
            Type::I64 => Type::I64,
            Type::F64 => Type::F64,
            Type::GcRef | Type::Unknown => Type::I32,
            _ => Type::I32,
        };
        let one = match &result_ty {
            Type::I64 => Expr::Int(1),
            Type::F64 => Expr::Float(1.0),
            _ => Expr::Int(1),
        };
        Ok(Expr::BinOp {
            op: BinOp::Add,
            left: Box::new(n),
            right: Box::new(one),
            ty: result_ty,
        })
    }

    /// (dec n) -> (- n 1)
    fn lower_dec(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 1 {
            return Err(CompileError::Parse("dec requires exactly 1 argument".into()));
        }
        // Arguments are never in tail position
        let n = self.with_tail_disabled(|l| l.lower_expr(&args[0]))?;
        let ty = n.expr_type();
        // GcRef means boxed value - treat as I32 for arithmetic
        let result_ty = match &ty {
            Type::I64 => Type::I64,
            Type::F64 => Type::F64,
            Type::GcRef | Type::Unknown => Type::I32,
            _ => Type::I32,
        };
        let one = match &result_ty {
            Type::I64 => Expr::Int(1),
            Type::F64 => Expr::Float(1.0),
            _ => Expr::Int(1),
        };
        Ok(Expr::BinOp {
            op: BinOp::Sub,
            left: Box::new(n),
            right: Box::new(one),
            ty: result_ty,
        })
    }

    /// (abs n) -> (if (< n 0) (- 0 n) n)
    fn lower_abs(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 1 {
            return Err(CompileError::Parse("abs requires exactly 1 argument".into()));
        }
        // Arguments are never in tail position
        let n = self.with_tail_disabled(|l| l.lower_expr(&args[0]))?;
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
        self.with_tail_disabled(|l| {
            // For 2+ args: (min a b c) -> (let [t1 a t2 b] (if (< t1 t2) (min t1 c) (min t2 c)))
            // Simplified: chain of binary comparisons
            let mut result = l.lower_expr(&args[0])?;
            let ty = result.expr_type();
            let result_ty = if ty == Type::Unknown { Type::I32 } else { ty };

            for arg in &args[1..] {
                let b = l.lower_expr(arg)?;

                // Store both in locals to avoid double evaluation
                let a_local = l.next_local;
                l.local_types.insert(a_local, result_ty.clone());
                l.next_local += 1;
                let b_local = l.next_local;
                l.local_types.insert(b_local, result_ty.clone());
                l.next_local += 1;

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

            Ok(result)
        })
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
        self.with_tail_disabled(|l| {
            let mut result = l.lower_expr(&args[0])?;
            let ty = result.expr_type();
            let result_ty = if ty == Type::Unknown { Type::I32 } else { ty };

            for arg in &args[1..] {
                let b = l.lower_expr(arg)?;

                let a_local = l.next_local;
                l.local_types.insert(a_local, result_ty.clone());
                l.next_local += 1;
                let b_local = l.next_local;
                l.local_types.insert(b_local, result_ty.clone());
                l.next_local += 1;

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
            Ok(result)
        })
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
        let parts = self.with_tail_disabled(|l| {
            args.iter()
                .map(|e| l.lower_expr(e))
                .collect::<CompileResult<Vec<_>>>()
        })?;
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
                let (result, was_tail) = self.with_args_context(|l| {
                    args.iter()
                        .map(|e| l.lower_expr(e))
                        .collect::<CompileResult<Vec<_>>>()
                });
                let lowered_args = result?;

                // Imports can be tail calls too
                return if was_tail {
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
            let (result, was_tail) = self.with_args_context(|l| {
                args.iter()
                    .map(|e| l.lower_expr(e))
                    .collect::<CompileResult<Vec<_>>>()
            });
            let lowered_args = result?;

            return if was_tail {
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
            let (result, was_tail) = self.with_args_context(|l| {
                args.iter()
                    .map(|e| l.lower_expr(e))
                    .collect::<CompileResult<Vec<_>>>()
            });
            let lowered_args = result?;

            if was_tail {
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
        } else if let Some(&(local_idx, ref ty)) = self.locals.get(name) {
            // It's a local variable - could be a closure
            // Clone values before mutable borrow
            let ty = ty.clone();

            // Emit ClosureCall - use with_args_context to get was_tail
            let (result, was_tail) = self.with_args_context(|l| {
                args.iter()
                    .map(|e| l.lower_expr(e))
                    .collect::<CompileResult<Vec<_>>>()
            });
            let lowered_args = result?;

            Ok(Expr::ClosureCall {
                closure: Box::new(Expr::LocalGet {
                    local: local_idx,
                    ty,
                }),
                args: lowered_args,
                in_tail_position: was_tail,
            })
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
        let (coll, index) = self.with_tail_disabled(|l| {
            Ok((l.lower_expr(&args[0])?, l.lower_expr(&args[1])?))
        })?;

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
        let coll = self.with_tail_disabled(|l| l.lower_expr(&args[0]))?;

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
        let coll = self.with_tail_disabled(|l| l.lower_expr(&args[0]))?;

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

    /// Lower (conj coll val) -> SetConj, Cons, or ProtocolDispatch
    ///
    /// Vector conj now uses protocol dispatch (implemented in core.suss).
    /// Sets and Cons still use fast paths.
    fn lower_conj(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        use crate::ir::{gc_types, method_ids};

        if args.len() != 2 {
            return Err(CompileError::Parse(
                "conj requires exactly 2 arguments: collection and value".into(),
            ));
        }

        // Arguments are never in tail position
        let (coll, val) = self.with_tail_disabled(|l| {
            Ok((l.lower_expr(&args[0])?, l.lower_expr(&args[1])?))
        })?;

        // Fast path: if we know the collection type at compile time
        if let Some(type_id) = self.infer_collection_type(&args[0]) {
            return match type_id {
                // Vector conj now uses protocol dispatch (core.suss implementation)
                t if t == gc_types::PERSISTENT_VECTOR => Ok(Expr::ProtocolDispatch {
                    obj: Box::new(coll),
                    method_id: method_ids::CONJ,
                    args: vec![val],
                    in_tail_position: self.in_tail_position,
                }),
                t if t == gc_types::PERSISTENT_SET => Ok(Expr::ProtocolDispatch {
                    obj: Box::new(coll),
                    method_id: method_ids::CONJ,
                    args: vec![val],
                    in_tail_position: self.in_tail_position,
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

    // NOTE: lower_cons() removed - cons is now a regular function in core.suss
    // that calls the ->Cons constructor generated by deftype.

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
        let coll = self.with_tail_disabled(|l| l.lower_expr(&args[0]))?;

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
        let (coll, key) = self.with_tail_disabled(|l| {
            Ok((l.lower_expr(&args[0])?, l.lower_expr(&args[1])?))
        })?;

        // Fast path: if we know the collection type at compile time
        if let Some(type_id) = self.infer_collection_type(&args[0]) {
            return match type_id {
                t if t == gc_types::PERSISTENT_MAP => {
                    // Use protocol dispatch for maps (MapGet has local variable issues)
                    Ok(Expr::ProtocolDispatch {
                        obj: Box::new(coll),
                        method_id: method_ids::LOOKUP,
                        args: vec![key],
                        in_tail_position: self.in_tail_position,
                    })
                }
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

    /// Lower (assoc map key val) -> ProtocolDispatch for IAssociative/-assoc
    fn lower_assoc(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 3 {
            return Err(CompileError::Parse(
                "assoc requires exactly 3 arguments: map, key, and value".into(),
            ));
        }
        let map = self.lower_expr(&args[0])?;
        let key = self.lower_expr(&args[1])?;
        let val = self.lower_expr(&args[2])?;
        Ok(Expr::ProtocolDispatch {
            obj: Box::new(map),
            method_id: method_ids::ASSOC,
            args: vec![key, val],
            in_tail_position: self.in_tail_position,
        })
    }

    /// Lower (contains? set key) using protocol dispatch
    /// Desugars to: (if (nil? (-lookup set key)) false true)
    fn lower_contains(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        use crate::ir::method_ids;

        if args.len() != 2 {
            return Err(CompileError::Parse(
                "contains? requires exactly 2 arguments: set and key".into(),
            ));
        }
        // Arguments to contains? are never in tail position
        let (set, key) = self.with_tail_disabled(|l| {
            Ok((l.lower_expr(&args[0])?, l.lower_expr(&args[1])?))
        })?;

        // Use protocol dispatch to call -lookup, then check if result is not nil
        let lookup = Expr::ProtocolDispatch {
            obj: Box::new(set),
            method_id: method_ids::LOOKUP,
            args: vec![key],
            in_tail_position: false,
        };

        // Check if the result is nil
        let nil_check = Expr::NilCheck(Box::new(lookup));

        // Return false if nil, true otherwise
        Ok(Expr::If {
            cond: Box::new(nil_check),
            then_branch: Box::new(Expr::Bool(false)),
            else_branch: Box::new(Expr::Bool(true)),
            ty: Type::GcRef, // Returns boxed boolean
        })
    }

    /// Lower (disj set val) -> SetDisj
    fn lower_disj(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 2 {
            return Err(CompileError::Parse(
                "disj requires exactly 2 arguments: set and value".into(),
            ));
        }
        // Arguments are never in tail position
        let (set, val) = self.with_tail_disabled(|l| {
            Ok((l.lower_expr(&args[0])?, l.lower_expr(&args[1])?))
        })?;
        Ok(Expr::SetDisj {
            set: Box::new(set),
            val: Box::new(val),
        })
    }

    /// Lower (dissoc map key) -> MapDissoc
    fn lower_dissoc(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 2 {
            return Err(CompileError::Parse(
                "dissoc requires exactly 2 arguments: map and key".into(),
            ));
        }
        // Arguments are never in tail position
        let (map, key) = self.with_tail_disabled(|l| {
            Ok((l.lower_expr(&args[0])?, l.lower_expr(&args[1])?))
        })?;
        Ok(Expr::MapDissoc {
            map: Box::new(map),
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

    // ========================================================================
    // Built-in Function Wrappers
    // ========================================================================

    /// Returns the arity of a built-in function if it exists.
    /// Used to detect built-ins in value position.
    fn builtin_arity(&self, name: &str) -> Option<u32> {
        match name {
            // Variadic arithmetic (handled specially when used as values)
            "+" | "-" | "*" | "/" => Some(2), // default arity for direct calls
            // Binary arithmetic
            "rem" | "mod" => Some(2),
            // Binary comparison
            "=" | "not=" | "<" | "<=" | ">" | ">=" => Some(2),
            // Unary
            "inc" | "dec" | "not" => Some(1),
            _ => None,
        }
    }

    /// Check if a built-in is variadic (supports 0-8 args via apply).
    fn is_variadic_builtin(&self, name: &str) -> bool {
        matches!(name, "+" | "-" | "*" | "/")
    }

    /// Create a closure wrapper for a built-in function used in value position.
    /// Example: `(let [f +] (f 1 2))` - here `+` is in value position.
    fn lower_builtin_as_closure(&mut self, name: &str, arity: u32) -> CompileResult<Expr> {
        // For variadic builtins, create a VARIADIC_CLOSURE instead
        if self.is_variadic_builtin(name) {
            return self.lower_variadic_builtin_as_closure(name);
        }

        // Check if we already have a wrapper for this built-in
        if let Some(&func_idx) = self.builtin_wrappers.get(name) {
            return Ok(Expr::ClosureNew {
                func_idx,
                arity,
                captures: vec![], // no captures needed for built-ins
            });
        }

        // Generate wrapper function name
        let wrapper_name = format!("$builtin_{}", name.replace(|c: char| !c.is_alphanumeric(), "_"));

        // Calculate wrapper function index
        let num_pending = self.pending_closures.len() as u32;
        let wrapper_idx = self.num_deftype_constructors + self.num_analyzed_funcs + self.num_extension_funcs + num_pending;

        // Register wrapper function
        self.func_indices.insert(wrapper_name.clone(), wrapper_idx);
        self.builtin_wrappers.insert(name.to_string(), wrapper_idx);

        // Generate wrapper body: a call expression to the built-in
        // For binary ops: (fn [a b] (+ a b))
        // For unary ops: (fn [a] (inc a))
        let param_names: Vec<String> = (0..arity)
            .map(|i| format!("$arg{}", i))
            .collect();

        let body = self.make_builtin_call_body(name, &param_names);

        // Store closure wrapper for later generation
        self.pending_closures.push(ClosureWrapper {
            name: wrapper_name,
            captures: vec![], // no captures
            params: param_names,
            body,
            is_variadic: false,
        });

        Ok(Expr::ClosureNew {
            func_idx: wrapper_idx,
            arity,
            captures: vec![], // no captures needed for built-ins
        })
    }

    /// Create a variadic closure for variadic builtins (+, *, -, /).
    /// Generates 9 wrapper functions (arities 0-8) and returns a VariadicClosureNew.
    fn lower_variadic_builtin_as_closure(&mut self, name: &str) -> CompileResult<Expr> {
        // Check if we already have wrappers for this variadic builtin
        // Use a special key for variadic wrappers
        let cache_key = format!("variadic_{}", name);
        if let Some(&base_idx) = self.builtin_wrappers.get(&cache_key) {
            // We stored the base index; the 9 funcs are at base_idx..base_idx+9
            let func_indices: [u32; 9] = std::array::from_fn(|i| base_idx + i as u32);
            return Ok(Expr::VariadicClosureNew {
                op: name.to_string(),
                func_indices,
            });
        }

        // Generate 9 wrapper functions (one per arity 0-8)
        let base_idx = self.num_deftype_constructors + self.num_analyzed_funcs + self.num_extension_funcs + self.pending_closures.len() as u32;

        for arity in 0..=8u32 {
            let wrapper_name = format!(
                "$variadic_{}_{}",
                name.replace(|c: char| !c.is_alphanumeric(), "_"),
                arity
            );

            // Calculate function index
            let func_idx = base_idx + arity;
            self.func_indices.insert(wrapper_name.clone(), func_idx);

            // Generate parameter names
            let param_names: Vec<String> = (0..arity)
                .map(|i| format!("$arg{}", i))
                .collect();

            // Generate body based on arity and operation
            let body = self.make_variadic_builtin_body(name, &param_names);

            // Store wrapper for later generation
            // Mark as variadic for special codegen (no env parameter)
            self.pending_closures.push(ClosureWrapper {
                name: wrapper_name,
                captures: vec![], // no captures
                params: param_names,
                body,
                is_variadic: true, // Variadic wrappers don't take env
            });
        }

        // Cache the base index for future lookups
        self.builtin_wrappers.insert(cache_key, base_idx);

        let func_indices: [u32; 9] = std::array::from_fn(|i| base_idx + i as u32);
        Ok(Expr::VariadicClosureNew {
            op: name.to_string(),
            func_indices,
        })
    }

    /// Create the body expression for a variadic built-in wrapper function.
    /// Handles special cases for 0-arg and 1-arg calls.
    fn make_variadic_builtin_body(&self, name: &str, params: &[String]) -> Edn {
        match (name, params.len()) {
            // Zero-arg cases
            ("+", 0) => Edn::Number(suss_core::Number::Integer(0.into())),
            ("*", 0) => Edn::Number(suss_core::Number::Integer(1.into())),
            ("-", 0) | ("/", 0) => {
                // These shouldn't happen in practice, but return 0 as fallback
                Edn::Number(suss_core::Number::Integer(0.into()))
            }

            // Single-arg cases
            ("+", 1) | ("*", 1) => {
                // Just return the argument
                Edn::Symbol(suss_core::Symbol::new(&params[0]))
            }
            ("-", 1) => {
                // Negate: (- 0 x)
                Edn::List(vec![
                    Edn::Symbol(suss_core::Symbol::new("-")),
                    Edn::Number(suss_core::Number::Integer(0.into())),
                    Edn::Symbol(suss_core::Symbol::new(&params[0])),
                ])
            }
            ("/", 1) => {
                // Reciprocal: (/ 1.0 x)
                Edn::List(vec![
                    Edn::Symbol(suss_core::Symbol::new("/")),
                    Edn::Number(suss_core::Number::Float(1.0)),
                    Edn::Symbol(suss_core::Symbol::new(&params[0])),
                ])
            }

            // Two+ args: chain the operations
            _ => self.make_builtin_call_body(name, params),
        }
    }

    /// Create the body expression for a built-in wrapper function.
    /// Returns an Edn expression like `(+ $arg0 $arg1)`.
    fn make_builtin_call_body(&self, name: &str, params: &[String]) -> Edn {
        let mut items = vec![Edn::Symbol(suss_core::Symbol::new(name))];
        for param in params {
            items.push(Edn::Symbol(suss_core::Symbol::new(param)));
        }
        Edn::List(items)
    }

    // =========================================================================
    // Protocol Lowering
    // =========================================================================

    /// Lower protocol definitions to IR ProtocolDefs.
    /// Assigns method IDs to each protocol method for each arity.
    /// Also records return type hints for protocol dispatch.
    fn lower_protocols(&mut self, protocols: &[AnalyzedProtocol]) -> CompileResult<()> {
        for protocol in protocols {
            let mut methods = Vec::new();

            for method in &protocol.methods {
                // Store return type hint if specified
                if let Some(ref return_type) = method.return_type {
                    self.method_return_types.insert(method.name.clone(), return_type.clone());
                }

                // Register each arity of this method
                for arity_params in &method.arities {
                    let arity = arity_params.len();
                    let method_id = self.get_or_assign_method_id(&protocol.name, &method.name, arity);
                    // Only add to methods list once (for the first arity we see)
                    if !methods.iter().any(|(name, _)| name == &method.name) {
                        methods.push((method.name.clone(), method_id));
                    }
                }
                // If no arities defined, this is an error in the protocol definition
                if method.arities.is_empty() {
                    return Err(CompileError::Semantic(format!(
                        "Protocol method {} has no arities defined",
                        method.name
                    )));
                }
            }

            self.module.protocols.push(ProtocolDef {
                name: protocol.name.clone(),
                methods,
            });
        }

        Ok(())
    }

    /// Get the method ID for a protocol method, or assign a new one if user-defined.
    fn get_or_assign_method_id(&mut self, protocol_name: &str, method_name: &str, arity: usize) -> u32 {
        // Check built-in methods first - they have fixed IDs regardless of arity
        if let Some(id) = self.builtin_method_id(method_name) {
            return id;
        }

        // Check if we already assigned an ID for this protocol/method/arity
        let key = format!("{}/{}/{}", protocol_name, method_name, arity);
        if let Some(&id) = self.user_method_ids.get(&key) {
            return id;
        }

        // Register this method in method_protocols for call-site lookup
        self.method_protocols
            .entry(method_name.to_string())
            .or_insert_with(Vec::new)
            .push((protocol_name.to_string(), arity));

        // Assign new ID
        let id = self.next_user_method_id;
        self.next_user_method_id += 1;
        self.user_method_ids.insert(key, id);
        id
    }

    /// Map built-in method names to their method IDs.
    fn builtin_method_id(&self, method_name: &str) -> Option<u32> {
        match method_name {
            "-lookup" => Some(method_ids::LOOKUP),
            "-assoc" => Some(method_ids::ASSOC),
            "-count" => Some(method_ids::COUNT),
            "-nth" => Some(method_ids::NTH),
            "-conj" => Some(method_ids::CONJ),
            "-first" => Some(method_ids::FIRST),
            "-rest" => Some(method_ids::REST),
            "-seq" => Some(method_ids::SEQ),
            "-hash" => Some(method_ids::HASH),
            "-equiv" => Some(method_ids::EQUIV),
            _ => None,
        }
    }

    /// Check if a name is a known protocol method.
    /// Uses the method_protocols map which is populated from defprotocol definitions.
    fn is_protocol_method(&self, name: &str) -> bool {
        self.method_protocols.contains_key(name)
    }

    /// Get the return type for a protocol method from ^type hints in defprotocol.
    /// Returns None if no type hint was specified (defaults to eqref/GcRef).
    fn get_protocol_method_return_type(&self, method_name: &str) -> Option<Type> {
        self.method_return_types.get(method_name).map(|hint| {
            match hint.as_str() {
                "i32" => Type::I32,
                "i64" => Type::I64,
                "f64" => Type::F64,
                "eqref" => Type::GcRef,
                _ => Type::GcRef, // Unknown hints default to GcRef
            }
        })
    }

    /// Lower direct protocol method calls: (-count obj), (-first seq), etc.
    /// These are dispatched via the protocol dispatch table at runtime.
    fn lower_protocol_method_call(&mut self, name: &str, args: &[Edn]) -> CompileResult<Expr> {
        if args.is_empty() {
            return Err(CompileError::Parse(format!(
                "Protocol method {} requires at least one argument (the object)",
                name
            )));
        }

        let arity = args.len();

        // Look up the method ID using arity
        let method_id = if let Some(id) = self.builtin_method_id(name) {
            // Built-in methods have fixed IDs regardless of arity
            id
        } else {
            // For user-defined protocol methods, look up by method name + arity
            // First find the protocol that defines this method with this arity
            let protocol_name = self.method_protocols.get(name)
                .and_then(|entries| {
                    entries.iter()
                        .find(|(_, a)| *a == arity)
                        .map(|(p, _)| p.clone())
                });

            match protocol_name {
                Some(protocol) => {
                    // Look up the method_id for this protocol/method/arity
                    let key = format!("{}/{}/{}", protocol, name, arity);
                    match self.user_method_ids.get(&key) {
                        Some(&id) => id,
                        None => {
                            return Err(CompileError::Undefined(format!(
                                "Protocol method {} with arity {} not implemented",
                                name, arity
                            )));
                        }
                    }
                }
                None => {
                    return Err(CompileError::Undefined(format!(
                        "Unknown protocol method {} with arity {}",
                        name, arity
                    )));
                }
            }
        };

        // First argument is the object to dispatch on, remaining arguments are method args
        let (result, was_tail) = self.with_args_context(|l| {
            let obj = l.lower_expr(&args[0])?;
            let method_args: Vec<Expr> = args[1..]
                .iter()
                .map(|e| l.lower_expr(e))
                .collect::<CompileResult<_>>()?;
            Ok((obj, method_args))
        });
        let (obj, method_args) = result?;

        Ok(Expr::ProtocolDispatch {
            obj: Box::new(obj),
            method_id,
            args: method_args,
            in_tail_position: was_tail,
        })
    }

    /// Lower deftype declarations.
    /// Assigns GC type indices, generates constructors, handles protocol implementations.
    fn lower_deftypes(&mut self, deftypes: &[AnalyzedDeftype]) -> CompileResult<()> {
        for deftype in deftypes {
            // Determine GC type index
            let gc_type_idx = if let Some(reserved) = deftype.reserved_type_id {
                // Use reserved index - this is for bootstrap types like BitmapIndexedNode
                reserved
            } else {
                let idx = self.next_user_gc_type;
                self.next_user_gc_type += 1;
                idx
            };

            // Determine runtime type ID
            let type_id = if deftype.reserved_type_id.is_some() {
                // Reserved types use their GC type index as type ID
                gc_type_idx as i32
            } else {
                let id = self.next_user_type_id;
                self.next_user_type_id += 1;
                id
            };

            // Convert fields
            let fields: Vec<UserTypeField> = deftype.fields.iter().map(|f| {
                let field_type = match f.type_hint.as_deref() {
                    Some("i32") => FieldType::I32,
                    Some("i64") => FieldType::I64,
                    Some("f64") => FieldType::F64,
                    Some("eqref") | None => FieldType::GcRef,
                    Some(other) => {
                        // This shouldn't happen if analysis is correct
                        eprintln!("Warning: unknown type hint '{}', using eqref", other);
                        FieldType::GcRef
                    }
                };
                UserTypeField {
                    name: f.name.clone(),
                    field_type,
                }
            }).collect();

            // Assign dispatch table slot (used for protocol dispatch)
            let dispatch_slot = self.next_dispatch_slot;
            self.next_dispatch_slot += 1;

            // Register in user_types map
            self.user_types.insert(deftype.name.clone(), UserTypeInfo {
                gc_type_idx,
                type_id,
                dispatch_slot,
                fields: fields.clone(),
            });

            // Create DeftypeDef for codegen
            let deftype_def = DeftypeDef {
                name: deftype.name.clone(),
                fields: fields.iter().map(|f| DeftypeFieldDef {
                    name: f.name.clone(),
                    field_type: f.field_type,
                }).collect(),
                gc_type_idx,
                type_id,
            };
            self.module.deftypes.push(deftype_def);

            // Generate constructor function ->TypeName
            // Skip constructors for HAMT node types (gc_type_idx 5-7) since they use
            // specific array types (ref $TRIE_NODE) that require special handling
            if gc_type_idx < gc_types::BITMAP_INDEXED_NODE || gc_type_idx > gc_types::HASH_COLLISION_NODE {
                self.lower_deftype_constructor(&deftype.name, gc_type_idx, type_id, &fields)?;
            }

            // Handle protocol implementations (similar to extend-type)
            for impl_ in &deftype.implementations {
                for method in &impl_.methods {
                    let arity = method.params.len();
                    let method_id = self.get_or_assign_method_id(&impl_.protocol_name, &method.name, arity);

                    // Generate wrapper function for this method implementation
                    let func_idx = self.lower_protocol_method_impl(
                        &deftype.name,
                        &impl_.protocol_name,
                        method,
                    )?;

                    // Add dispatch table entry using dispatch_slot
                    self.module.dispatch_entries.push(DispatchEntry {
                        dispatch_slot,
                        method_id,
                        func_idx,
                    });
                }
            }
        }

        Ok(())
    }

    /// Generate constructor function for a deftype: (fn [x y] (struct.new $Type type_id x y))
    fn lower_deftype_constructor(
        &mut self,
        type_name: &str,
        gc_type_idx: u32,
        type_id: i32,
        fields: &[UserTypeField],
    ) -> CompileResult<()> {
        let constructor_name = format!("->{}", type_name);

        // Calculate function index - constructors come first, before analyzed functions
        // This matches the order in which functions are added to module.functions:
        // 1. deftype constructors (added in lower_deftypes)
        // 2. analyzed functions (added after lower_deftypes)
        // 3. closures (added after analyzed functions)
        // Runtime helpers are emitted in both modes
        let func_idx = self.num_imports + gc_types::USER_FUNC_OFFSET + self.module.functions.len() as u32;

        self.func_indices.insert(constructor_name.clone(), func_idx);

        // Reset locals for new function
        self.locals.clear();
        self.local_types.clear();
        self.next_local = 0;

        // Set up parameters - one per field
        let mut params = Vec::new();
        for field in fields {
            let idx = self.next_local;
            self.locals.insert(field.name.clone(), (idx, Type::GcRef));
            self.local_types.insert(idx, Type::GcRef);
            self.next_local += 1;
            params.push((field.name.clone(), Type::GcRef));
        }

        // Build struct.new expression
        // Field 0 is always type_id (raw i32)
        let mut struct_fields = vec![Expr::RawI32(type_id)];

        // Add field values from parameters
        for (idx, field) in fields.iter().enumerate() {
            let local_get = Expr::LocalGet {
                local: idx as u32,
                ty: Type::GcRef,
            };

            // For primitive types, unbox the input value
            let field_value = match field.field_type {
                FieldType::I32 => {
                    // Unbox tagged integer to raw i32 for struct field
                    Expr::Unbox32(Box::new(local_get))
                }
                FieldType::I64 => {
                    // Unbox: (struct.get $LARGE_INT 1 (ref.cast ... arg))
                    Expr::StructGet {
                        type_idx: gc_types::LARGE_INT,
                        field_idx: gc_types::LI_VALUE,
                        value: Box::new(local_get),
                    }
                }
                FieldType::F64 => {
                    // Unbox: (struct.get $FLOAT 1 (ref.cast ... arg))
                    Expr::StructGet {
                        type_idx: gc_types::FLOAT,
                        field_idx: gc_types::FL_VALUE,
                        value: Box::new(local_get),
                    }
                }
                FieldType::GcRef => {
                    // No unboxing needed
                    local_get
                }
            };

            struct_fields.push(field_value);
        }

        let body = Expr::StructNew {
            type_idx: gc_type_idx,
            fields: struct_fields,
        };

        let function = Function {
            name: constructor_name,
            exported: false,
            export_name: None,
            params,
            return_type: Type::GcRef,
            has_explicit_return_type: false,
            locals: vec![Type::GcRef; fields.len()],
            body,
        };

        self.module.functions.push(function);

        Ok(())
    }

    /// Lower extend-type declarations to dispatch table entries.
    /// Each method implementation becomes a wrapper function registered in the dispatch table.
    fn lower_extensions(&mut self, extensions: &[AnalyzedExtension]) -> CompileResult<()> {
        for extension in extensions {
            // Get the dispatch slot for this type from user_types
            let dispatch_slot = self.type_name_to_dispatch_slot(&extension.type_name)?;

            for impl_ in &extension.implementations {
                for method in &impl_.methods {
                    let arity = method.params.len();
                    let method_id = self.get_or_assign_method_id(&impl_.protocol_name, &method.name, arity);

                    // Generate wrapper function for this method implementation
                    let func_idx = self.lower_protocol_method_impl(
                        &extension.type_name,
                        &impl_.protocol_name,
                        method,
                    )?;

                    // Add dispatch table entry using dispatch_slot
                    self.module.dispatch_entries.push(DispatchEntry {
                        dispatch_slot,
                        method_id,
                        func_idx,
                    });
                }
            }
        }

        Ok(())
    }

    /// Convert a type name to its runtime type ID.
    fn type_name_to_type_id(&self, type_name: &str) -> CompileResult<u32> {
        // Check primitive types first (not deftypes)
        match type_name {
            "String" => return Ok(type_ids::STRING as u32),
            "LargeInt" => return Ok(type_ids::LARGE_INT as u32),
            "Float" => return Ok(type_ids::FLOAT as u32),
            _ => {}
        }

        // Check user-defined types (includes collection types from core.suss:
        // Cons, PersistentVector, PersistentMap, PersistentSet, HAMT nodes, etc.)
        if let Some(info) = self.user_types.get(type_name) {
            return Ok(info.type_id as u32);
        }

        Err(CompileError::Undefined(format!(
            "Unknown type for protocol extension: {}",
            type_name
        )))
    }

    /// Convert a type name to its dispatch table slot.
    ///
    /// Slot mapping:
    /// - Primitive types (String, etc.): slots 0-4
    /// - User deftypes: slots 5+ (in definition order)
    fn type_name_to_dispatch_slot(&self, type_name: &str) -> CompileResult<u32> {
        // Check primitive types first (slots 0-4)
        // Note: These match the constants in generate_get_type_id_func
        match type_name {
            "Int64" | "LargeInt" => return Ok(0),
            "Float64" | "Float" => return Ok(1),
            "String" => return Ok(2),
            "Array" => return Ok(3),
            "I32Array" => return Ok(4),
            _ => {}
        }

        // Check user-defined types
        if let Some(info) = self.user_types.get(type_name) {
            return Ok(info.dispatch_slot);
        }

        Err(CompileError::Undefined(format!(
            "Unknown type for protocol extension: {}",
            type_name
        )))
    }

    /// Lower a protocol method implementation to a wrapper function.
    /// Returns the function index.
    fn lower_protocol_method_impl(
        &mut self,
        type_name: &str,
        protocol_name: &str,
        method: &AnalyzedMethodImpl,
    ) -> CompileResult<u32> {
        // Generate unique function name
        let func_name = format!(
            "$protocol_{}_{}_{}",
            type_name,
            protocol_name.replace(['/', '-'], "_"),
            method.name.replace('-', "_")
        );

        // Calculate function index for func_indices (with all offsets for call resolution)
        // Runtime helpers are emitted in both modes
        let full_func_idx = self.num_imports
            + gc_types::USER_FUNC_OFFSET
            + self.pending_closures.len() as u32
            + self.module.functions.len() as u32;

        // Register function with full index for call resolution
        self.func_indices.insert(func_name.clone(), full_func_idx);

        // For dispatch entries, store the index into ir.functions array
        // Codegen will add offsets via user_func_idx()
        let ir_func_idx = self.module.functions.len() as u32;

        // Reset locals for new function
        self.locals.clear();
        self.local_types.clear();
        self.next_local = 0;

        // Set up parameters - protocol methods take (self, args...)
        // All parameters are eqref
        let mut params = Vec::new();
        for param_name in &method.params {
            let idx = self.next_local;
            self.locals.insert(param_name.clone(), (idx, Type::GcRef));
            self.local_types.insert(idx, Type::GcRef);
            self.next_local += 1;
            params.push((param_name.clone(), Type::GcRef));
        }

        // Lower the body with self type context for field access resolution
        self.current_self_type = Some(type_name.to_string());
        self.in_tail_position = true;
        let body = self.lower_expr(&method.body)?;
        self.in_tail_position = false;
        self.current_self_type = None;

        // Determine return type from protocol definition (via type hints)
        // Falls back to GcRef if no hint provided
        let (return_type, has_explicit_return_type) =
            match self.get_protocol_method_return_type(&method.name) {
                Some(ty) => (ty, true),
                None => (Type::GcRef, false),
            };

        // Create function
        let function = Function {
            name: func_name,
            params,
            return_type,
            has_explicit_return_type,
            locals: self.collect_locals(),
            body,
            exported: false,
            export_name: None,
        };

        self.module.functions.push(function);

        Ok(ir_func_idx)
    }

    /// Collect local variable types for the current function.
    fn collect_locals(&self) -> Vec<Type> {
        let mut locals: Vec<_> = self.locals.iter()
            .map(|(_, (idx, ty))| (*idx, ty.clone()))
            .collect();
        locals.sort_by_key(|(idx, _)| *idx);
        locals.into_iter()
            .map(|(_, ty)| ty)
            .collect()
    }

    // ========================================================================
    // Array operations (ClojureScript-style primitives)
    // ========================================================================

    /// Lower (aget arr idx) -> ArrayGet
    /// Gets an element from a TRIE_NODE array at the given index.
    fn lower_aget(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 2 {
            return Err(CompileError::Parse(
                "aget requires exactly 2 arguments: array and index".into(),
            ));
        }
        let (array, index) = self.with_tail_disabled(|l| {
            Ok((l.lower_expr(&args[0])?, l.lower_expr(&args[1])?))
        })?;

        // Use TRIE_NODE as the default array type
        Ok(Expr::ArrayGet {
            type_idx: gc_types::TRIE_NODE,
            array: Box::new(array),
            index: Box::new(index),
        })
    }

    /// Lower (aset arr idx val) -> ArraySet
    /// Sets an element in a TRIE_NODE array at the given index.
    fn lower_aset(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 3 {
            return Err(CompileError::Parse(
                "aset requires exactly 3 arguments: array, index, and value".into(),
            ));
        }
        let (array, index, value) = self.with_tail_disabled(|l| {
            Ok((
                l.lower_expr(&args[0])?,
                l.lower_expr(&args[1])?,
                l.lower_expr(&args[2])?,
            ))
        })?;

        // Use TRIE_NODE as the default array type
        Ok(Expr::ArraySet {
            type_idx: gc_types::TRIE_NODE,
            array: Box::new(array),
            index: Box::new(index),
            value: Box::new(value),
        })
    }

    /// Lower (alength arr) -> ArrayLen
    /// Gets the length of a TRIE_NODE array.
    fn lower_alength(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 1 {
            return Err(CompileError::Parse(
                "alength requires exactly 1 argument: array".into(),
            ));
        }
        let array = self.with_tail_disabled(|l| l.lower_expr(&args[0]))?;

        Ok(Expr::ArrayLen(Box::new(array)))
    }

    /// Lower (aclone arr) -> ArrayClone
    /// Creates a shallow copy of a TRIE_NODE array.
    fn lower_aclone(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 1 {
            return Err(CompileError::Parse(
                "aclone requires exactly 1 argument: array".into(),
            ));
        }
        let array = self.with_tail_disabled(|l| l.lower_expr(&args[0]))?;

        // Use TRIE_NODE as the default array type
        Ok(Expr::ArrayClone {
            type_idx: gc_types::TRIE_NODE,
            array: Box::new(array),
        })
    }

    /// Lower (acopy dst dst-offset src src-offset len) -> ArrayCopy
    /// Copies elements between TRIE_NODE arrays. Returns nil.
    fn lower_acopy(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 5 {
            return Err(CompileError::Parse(
                "acopy requires exactly 5 arguments: dst dst-offset src src-offset len".into(),
            ));
        }
        let dst = self.with_tail_disabled(|l| l.lower_expr(&args[0]))?;
        let dst_offset = self.with_tail_disabled(|l| l.lower_expr(&args[1]))?;
        let src = self.with_tail_disabled(|l| l.lower_expr(&args[2]))?;
        let src_offset = self.with_tail_disabled(|l| l.lower_expr(&args[3]))?;
        let len = self.with_tail_disabled(|l| l.lower_expr(&args[4]))?;

        // Use TRIE_NODE as the default array type
        Ok(Expr::ArrayCopy {
            type_idx: gc_types::TRIE_NODE,
            dst: Box::new(dst),
            dst_offset: Box::new(dst_offset),
            src: Box::new(src),
            src_offset: Box::new(src_offset),
            len: Box::new(len),
        })
    }

    /// Lower (make-array size) -> ArrayNewDefault
    /// Creates a new TRIE_NODE array with null values.
    fn lower_make_array(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 1 {
            return Err(CompileError::Parse(
                "make-array requires exactly 1 argument: size".into(),
            ));
        }
        let size = self.with_tail_disabled(|l| l.lower_expr(&args[0]))?;

        // Use TRIE_NODE as the default array type
        Ok(Expr::ArrayNewDefault {
            type_idx: gc_types::TRIE_NODE,
            size: Box::new(size),
        })
    }

    /// Lower (bit-count x) -> BitCount
    /// Returns the population count (number of 1 bits) of an integer.
    fn lower_bit_count(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 1 {
            return Err(CompileError::Parse(
                "bit-count requires exactly 1 argument".into(),
            ));
        }
        let value = self.with_tail_disabled(|l| l.lower_expr(&args[0]))?;

        Ok(Expr::BitCount(Box::new(value)))
    }
}
