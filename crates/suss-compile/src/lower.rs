//! Lowering from analyzed Suss to IR
//!
//! Converts analyzed s-expressions into the IR representation.

use std::collections::HashMap;

use num_traits::ToPrimitive;
use suss_core::{Edn, Keyword, Number, Symbol};

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
    /// Parameter names (excluding env, excluding rest param)
    params: Vec<String>,
    /// Rest parameter name (for variadic functions)
    rest_param: Option<String>,
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
    is_mutable: bool,
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
    /// Number of protocol method implementations from inline deftype protocols
    /// Used to calculate closure wrapper indices correctly
    num_deftype_impl_funcs: u32,
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
    /// Variadic functions: name -> min_arity (number of fixed params before &)
    /// Used to package arguments at call sites
    variadic_funcs: HashMap<String, usize>,
    /// Function arities: name -> number of parameters
    /// Used for creating closure wrappers in #'var
    func_arities: HashMap<String, usize>,

    // ========================================================================
    // Namespace Resolution (Phase 5)
    // ========================================================================

    /// Current namespace being compiled
    current_ns: Option<String>,
    /// Namespace aliases: alias -> full namespace name
    /// From (require '[myapp.utils :as utils])
    ns_aliases: HashMap<String, String>,
    /// Referred symbols: symbol_name -> source namespace
    /// From (require '[myapp.utils :refer [helper]])
    referred_symbols: HashMap<String, String>,
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
            num_deftype_impl_funcs: 0,
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
            variadic_funcs: HashMap::new(),
            func_arities: HashMap::new(),
            // Namespace resolution
            current_ns: None,
            ns_aliases: HashMap::new(),
            referred_symbols: HashMap::new(),
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

    /// Emit a function call, handling variadic functions and tail calls.
    fn emit_func_call(&mut self, idx: u32, name: &str, args: &[Edn]) -> CompileResult<Expr> {
        // Check if this is a variadic function
        if let Some(&_min_arity) = self.variadic_funcs.get(name) {
            // Variadic function: package all args into an array
            let (result, was_tail) = self.with_args_context(|l| {
                args.iter()
                    .map(|e| l.lower_expr(e))
                    .collect::<CompileResult<Vec<_>>>()
            });
            let lowered_args = result?;

            // Create an array containing all arguments
            let args_array = Expr::ArrayNew {
                type_idx: gc_types::ARRAY,
                elements: lowered_args,
            };

            // Call with single args_array argument
            if was_tail {
                Ok(Expr::TailCall {
                    func: idx,
                    args: vec![args_array],
                })
            } else {
                Ok(Expr::Call {
                    func: idx,
                    args: vec![args_array],
                })
            }
        } else {
            // Non-variadic function: pass args directly
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
        }
    }

    /// Resolve an unqualified function name to a qualified name.
    ///
    /// Resolution order:
    /// 1. Current namespace (if set)
    /// 2. Referred symbols from requires
    /// 3. suss.core (implicit require)
    /// 4. Unqualified name (for backward compatibility)
    fn resolve_func_name(&self, name: &str) -> Option<(String, u32)> {
        // 1. Check current namespace first
        if let Some(ref ns) = self.current_ns {
            let qualified = format!("{}/{}", ns, name);
            if let Some(&idx) = self.func_indices.get(&qualified) {
                return Some((qualified, idx));
            }
        }

        // 2. Check referred symbols
        if let Some(source_ns) = self.referred_symbols.get(name) {
            let qualified = format!("{}/{}", source_ns, name);
            if let Some(&idx) = self.func_indices.get(&qualified) {
                return Some((qualified, idx));
            }
        }

        // 3. Check suss.core (implicit require)
        let core_qualified = format!("suss.core/{}", name);
        if let Some(&idx) = self.func_indices.get(&core_qualified) {
            return Some((core_qualified, idx));
        }

        // 4. Fall back to unqualified name (backward compatibility)
        if let Some(&idx) = self.func_indices.get(name) {
            return Some((name.to_string(), idx));
        }

        None
    }

    /// Populate namespace aliases and referred symbols from requires.
    fn populate_require_bindings(&mut self, requires: &[crate::analyze::AnalyzedRequire]) {
        use crate::analyze::RequireSource;

        for req in requires {
            match &req.source {
                RequireSource::SussNamespace { namespace } => {
                    // Add alias if specified
                    if let Some(ref alias) = req.alias {
                        self.ns_aliases.insert(alias.clone(), namespace.clone());
                    }

                    // Add referred symbols
                    for sym in &req.refers {
                        self.referred_symbols.insert(sym.clone(), namespace.clone());
                    }

                    // Note: refer_all is handled during multi-namespace compilation
                    // when we have the full NamespaceRegistry available
                }
                RequireSource::WitInterface { .. } => {
                    // WASI imports are handled separately via import_indices
                }
            }
        }
    }

    fn lower_module(&mut self, analyzed: &AnalyzedModule) -> CompileResult<Module> {
        // Set up namespace resolution info
        self.current_ns = analyzed.namespace.clone();
        self.populate_require_bindings(&analyzed.suss_requires);

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
        // Also handles inline protocol implementations
        self.lower_deftypes(&analyzed.deftypes)?;

        // Count deftype constructors (skip HAMT nodes 5-7 which don't generate constructors)
        self.num_deftype_constructors = analyzed
            .deftypes
            .iter()
            .filter(|dt| {
                use crate::ir::gc_types;
                let gc_type_idx = dt.reserved_type_id.unwrap_or(gc_types::NUM_GC_TYPES);
                gc_type_idx < gc_types::BITMAP_INDEXED_NODE || gc_type_idx > gc_types::HASH_COLLISION_NODE
            })
            .count() as u32;

        // Count inline protocol implementations from deftype declarations
        self.num_deftype_impl_funcs = analyzed
            .deftypes
            .iter()
            .flat_map(|dt| dt.implementations.iter())
            .map(|impl_| impl_.methods.len() as u32)
            .sum();

        let num_deftype_funcs = self.num_deftype_constructors + self.num_deftype_impl_funcs;

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
        //   [N+H+C..N+H+C+I)                - Deftype inline protocol impls
        //   [N+H+C+I..N+H+C+I+F)            - User functions (core.sus + user code)
        //
        // Where: N = num_imports, H = NUM_RUNTIME_HELPERS (2),
        //        C = num_deftype_constructors, I = num_deftype_impl_funcs
        //
        // IMPORTANT: Both REPL and Component modes use the same offset because runtime
        // helpers are emitted in both modes. This ensures function calls work correctly
        // in both contexts.
        use crate::ir::gc_types;
        let func_offset = gc_types::USER_FUNC_OFFSET;
        for (idx, func) in analyzed.functions.iter().enumerate() {
            let func_idx = self.num_imports + func_offset + num_deftype_funcs + idx as u32;

            // Namespace-qualify the function name if there's a namespace
            let func_name = if let Some(ref ns) = analyzed.namespace {
                format!("{}/{}", ns, func.name)
            } else {
                func.name.clone()
            };

            self.func_indices.insert(func_name.clone(), func_idx);
            // Track function arity for #'var closure wrappers
            self.func_arities.insert(func_name.clone(), func.params.len());
            // Track variadic functions for call site handling
            if func.rest_param.is_some() {
                self.variadic_funcs.insert(func_name, func.params.len());
            }
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

        // Calculate methods_per_type for dispatch table sizing
        // This ensures the table can accommodate all method IDs (built-in + user-defined)
        self.module.methods_per_type = self.module.max_method_id + 1;

        Ok(std::mem::take(&mut self.module))
    }

    /// Generate a wrapper function for a closure
    /// Regular closure signature: (env: eqref, params...) -> eqref
    /// Variadic closure signature: (env: eqref, args_array: eqref) -> eqref
    fn lower_closure_wrapper(&mut self, closure: &ClosureWrapper) -> CompileResult<Function> {
        use crate::ir::gc_types;

        // Reset locals for new function
        self.locals.clear();
        self.local_types.clear();
        self.next_local = 0;

        let mut params = Vec::new();

        // All closures have env as first parameter
        let env_idx = self.next_local;
        self.next_local += 1;
        self.local_types.insert(env_idx, Type::GcRef);
        params.push(("$env".to_string(), Type::GcRef));

        // Bindings to wrap the body with
        let mut all_bindings = Vec::new();

        // IMPORTANT: Add function params BEFORE captured variables
        // In WASM, params occupy the first local indices, so we must reserve them first
        if closure.rest_param.is_some() {
            // Variadic closure: second param is args_array
            let args_array_local = self.next_local;
            self.locals.insert("__args".to_string(), (args_array_local, Type::GcRef));
            self.local_types.insert(args_array_local, Type::GcRef);
            self.next_local += 1;
            params.push(("__args".to_string(), Type::GcRef));

            // Now add captured variables as synthetic locals that read from env
            for (cap_idx, cap_name) in closure.captures.iter().enumerate() {
                let local_idx = self.next_local;
                self.locals.insert(cap_name.clone(), (local_idx, Type::GcRef));
                self.local_types.insert(local_idx, Type::GcRef);
                self.next_local += 1;

                // env[cap_idx] - ArrayGet from env
                let env_get = Expr::ArrayGet {
                    type_idx: gc_types::ARRAY,
                    array: Box::new(Expr::LocalGet {
                        local: env_idx,
                        ty: Type::GcRef,
                    }),
                    index: Box::new(Expr::Int(cap_idx as i64)),
                };
                all_bindings.push((local_idx, env_get));
            }

            // Extract fixed params from args_array
            for (i, param_name) in closure.params.iter().enumerate() {
                let local_idx = self.next_local;
                self.locals.insert(param_name.clone(), (local_idx, Type::GcRef));
                self.local_types.insert(local_idx, Type::GcRef);
                self.next_local += 1;

                let aget_expr = Expr::ArrayGet {
                    type_idx: gc_types::ARRAY,
                    array: Box::new(Expr::LocalGet {
                        local: args_array_local,
                        ty: Type::GcRef,
                    }),
                    index: Box::new(Expr::Int(i as i64)),
                };
                all_bindings.push((local_idx, aget_expr));
            }

            // Extract rest param
            let rest_param_name = closure.rest_param.as_ref().unwrap();
            let rest_local_idx = self.next_local;
            self.locals.insert(rest_param_name.clone(), (rest_local_idx, Type::GcRef));
            self.local_types.insert(rest_local_idx, Type::GcRef);
            self.next_local += 1;

            let fixed_count = closure.params.len();
            let rest_expr = if fixed_count == 0 {
                // Rest is entire args array
                Expr::LocalGet {
                    local: args_array_local,
                    ty: Type::GcRef,
                }
            } else {
                // Extract rest using array slice
                self.generate_rest_param_extraction(args_array_local, fixed_count)?
            };
            all_bindings.push((rest_local_idx, rest_expr));
        } else {
            // Non-variadic: add regular parameters first
            for param_name in &closure.params {
                let idx = self.next_local;
                self.locals.insert(param_name.clone(), (idx, Type::GcRef));
                self.local_types.insert(idx, Type::GcRef);
                self.next_local += 1;
                params.push((param_name.clone(), Type::GcRef));
            }

            // Now add captured variables as synthetic locals that read from env
            for (cap_idx, cap_name) in closure.captures.iter().enumerate() {
                let local_idx = self.next_local;
                self.locals.insert(cap_name.clone(), (local_idx, Type::GcRef));
                self.local_types.insert(local_idx, Type::GcRef);
                self.next_local += 1;

                // env[cap_idx] - ArrayGet from env
                let env_get = Expr::ArrayGet {
                    type_idx: gc_types::ARRAY,
                    array: Box::new(Expr::LocalGet {
                        local: env_idx,
                        ty: Type::GcRef,
                    }),
                    index: Box::new(Expr::Int(cap_idx as i64)),
                };
                all_bindings.push((local_idx, env_get));
            }
        }

        // Lower the body with all bindings in scope
        self.in_tail_position = true;
        let body_expr = self.lower_expr(&closure.body)?;
        self.in_tail_position = false;

        // Wrap body with bindings if any
        let final_body = if all_bindings.is_empty() {
            body_expr
        } else {
            Expr::Let {
                bindings: all_bindings,
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
            rest_param: closure.rest_param.clone(),
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

        // Check if this is a variadic function
        let is_variadic = func.rest_param.is_some();

        // For variadic functions, the actual WASM function takes a single args_array parameter.
        // We generate code to extract fixed params and rest param from this array.
        let (params, body) = if is_variadic {
            self.lower_variadic_function(func)?
        } else {
            // Non-variadic: add parameters as locals with their types
            let mut params = Vec::new();
            for (name, ty) in &func.params {
                let idx = self.next_local;
                self.locals.insert(name.clone(), (idx, ty.clone()));
                self.local_types.insert(idx, ty.clone());
                self.next_local += 1;
                params.push((name.clone(), ty.clone()));
            }

            // Lower body - function body is in tail position UNLESS:
            // - The function returns Unit (nothing to return)
            // - The function returns Result (WIT exit marshaling needed after body)
            let enable_tail = match &func.return_type {
                Type::Unit => false,
                Type::Result { .. } => false, // Need exit marshaling, no tail calls
                _ => true,
            };
            self.in_tail_position = enable_tail;
            let body = self.lower_expr(&func.body)?;
            self.in_tail_position = false;

            (params, body)
        };

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
            rest_param: func.rest_param.clone(),
            return_type,
            has_explicit_return_type,
            locals,
            body,
        })
    }

    /// Lower a variadic function.
    ///
    /// Variadic functions are compiled to take a single args_array parameter.
    /// The body is wrapped in a let that extracts fixed params using aget
    /// and creates the rest param using subvec.
    fn lower_variadic_function(
        &mut self,
        func: &AnalyzedFunction,
    ) -> CompileResult<(Vec<(String, Type)>, Expr)> {
        let rest_param_name = func.rest_param.as_ref().unwrap();
        let fixed_param_count = func.params.len();

        // The WASM function takes a single args_array parameter
        let args_array_local = self.next_local;
        self.locals.insert("__args".to_string(), (args_array_local, Type::GcRef));
        self.local_types.insert(args_array_local, Type::GcRef);
        self.next_local += 1;

        // Generate bindings to extract fixed params and rest param
        let mut bindings = Vec::new();

        // Extract fixed params: (aget __args 0), (aget __args 1), etc.
        for (i, (name, _ty)) in func.params.iter().enumerate() {
            let local_idx = self.next_local;
            self.locals.insert(name.clone(), (local_idx, Type::GcRef));
            self.local_types.insert(local_idx, Type::GcRef);
            self.next_local += 1;

            // aget __args i
            let aget_expr = Expr::ArrayGet {
                type_idx: gc_types::ARRAY,
                array: Box::new(Expr::LocalGet {
                    local: args_array_local,
                    ty: Type::GcRef,
                }),
                index: Box::new(Expr::Int(i as i64)),
            };
            bindings.push((local_idx, aget_expr));
        }

        // Create rest param using subvec: (subvec __args fixed_param_count)
        // For now, we'll use a simpler approach: rest param is the args array itself
        // if there are no fixed params, or we slice it
        let rest_local_idx = self.next_local;
        self.locals
            .insert(rest_param_name.clone(), (rest_local_idx, Type::GcRef));
        self.local_types.insert(rest_local_idx, Type::GcRef);
        self.next_local += 1;

        // TODO: Implement proper subvec. For now, if no fixed params, rest = args.
        // Otherwise, we need to create a view/slice of the array.
        let rest_expr = if fixed_param_count == 0 {
            // Rest is the entire args array
            Expr::LocalGet {
                local: args_array_local,
                ty: Type::GcRef,
            }
        } else {
            // For now, create a simple loop to build a new array with remaining elements
            // This is inefficient but works. TODO: Add proper subvec support.
            //
            // We'll generate: a new array containing elements from fixed_param_count onwards
            // Using ArrayNewDefault + loop to copy elements
            //
            // Actually, let's use a simpler approach for now: just pass the whole array
            // and document that the user should use `(drop N coll)` or similar.
            //
            // For MVP, let's just generate code to build a vector from remaining args.
            self.generate_rest_param_extraction(args_array_local, fixed_param_count)?
        };
        bindings.push((rest_local_idx, rest_expr));

        // Lower the body with all bindings in scope
        self.in_tail_position = func.return_type != Type::Unit;
        let inner_body = self.lower_expr(&func.body)?;
        self.in_tail_position = false;

        // Wrap body in let with all the bindings
        let body = Expr::Let {
            bindings,
            body: Box::new(inner_body),
        };

        // Return single args_array parameter
        let params = vec![("__args".to_string(), Type::GcRef)];

        Ok((params, body))
    }

    /// Generate code to extract rest parameters from args array.
    /// Creates a new WASM array containing elements from `start_idx` onwards.
    fn generate_rest_param_extraction(
        &mut self,
        args_local: u32,
        start_idx: usize,
    ) -> CompileResult<Expr> {
        // Generate IR equivalent to:
        // (let [len (alength __args)
        //       rest-len (- len start_idx)
        //       rest-arr (make-array rest-len)]
        //   (acopy rest-arr 0 args start_idx rest-len)
        //   rest-arr)

        // Allocate locals for intermediate values
        let len_local = self.next_local;
        self.locals.insert("__len".to_string(), (len_local, Type::GcRef));
        self.local_types.insert(len_local, Type::GcRef);
        self.next_local += 1;

        let rest_len_local = self.next_local;
        self.locals.insert("__rest_len".to_string(), (rest_len_local, Type::GcRef));
        self.local_types.insert(rest_len_local, Type::GcRef);
        self.next_local += 1;

        let rest_arr_local = self.next_local;
        self.locals.insert("__rest_arr".to_string(), (rest_arr_local, Type::GcRef));
        self.local_types.insert(rest_arr_local, Type::GcRef);
        self.next_local += 1;

        // len = (alength args)
        let args_ref = Expr::LocalGet {
            local: args_local,
            ty: Type::GcRef,
        };
        let len_expr = Expr::ArrayLen(Box::new(args_ref.clone()));

        // rest_len = (- len start_idx)
        let rest_len_expr = Expr::BinOp {
            op: BinOp::Sub,
            left: Box::new(Expr::LocalGet { local: len_local, ty: Type::GcRef }),
            right: Box::new(Expr::Int(start_idx as i64)),
            ty: Type::I32,
        };

        // rest_arr = (make-array rest_len)
        let rest_arr_expr = Expr::ArrayNewDefault {
            type_idx: gc_types::ARRAY,
            size: Box::new(Expr::LocalGet { local: rest_len_local, ty: Type::GcRef }),
        };

        // (acopy rest_arr 0 args start_idx rest_len)
        let copy_expr = Expr::ArrayCopy {
            type_idx: gc_types::ARRAY,
            dst: Box::new(Expr::LocalGet { local: rest_arr_local, ty: Type::GcRef }),
            dst_offset: Box::new(Expr::Int(0)),
            src: Box::new(args_ref),
            src_offset: Box::new(Expr::Int(start_idx as i64)),
            len: Box::new(Expr::LocalGet { local: rest_len_local, ty: Type::GcRef }),
        };

        // Build the let expression
        let bindings = vec![
            (len_local, len_expr),
            (rest_len_local, rest_len_expr),
            (rest_arr_local, rest_arr_expr),
        ];

        let body = Expr::Block(vec![
            copy_expr,
            Expr::LocalGet { local: rest_arr_local, ty: Type::GcRef },
        ]);

        Ok(Expr::Let {
            bindings,
            body: Box::new(body),
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

            Edn::Keyword(kw) => {
                let idx =
                    self.module
                        .intern_keyword(kw.namespace.as_deref(), &kw.name);
                let hash = gc_types::hash_keyword(kw.namespace.as_deref(), &kw.name);
                Ok(Expr::Keyword { idx, hash })
            }

            Edn::Symbol(sym) => {
                // Check if it's a local
                if let Some(&(idx, ref ty)) = self.locals.get(&sym.name) {
                    Ok(Expr::LocalGet { local: idx, ty: ty.clone() })
                } else if let Some(arity) = self.builtin_arity(&sym.name) {
                    // It's a built-in in value position - wrap it as a closure
                    self.lower_builtin_as_closure(&sym.name, arity)
                } else if let Some((resolved_name, func_idx)) = self.resolve_func_name(&sym.name) {
                    // It's a user-defined function in value position
                    let arity = self.func_arities.get(&resolved_name).copied().unwrap_or(0);
                    let is_variadic = self.variadic_funcs.contains_key(&resolved_name);
                    if is_variadic {
                        // Variadic functions: wrap as closure
                        self.lower_variadic_user_func_as_closure(&resolved_name, func_idx)
                    } else if arity == 0 {
                        // Zero-arg functions (e.g., plain def values): call directly
                        // This makes (def x 42) work - referencing x calls the thunk
                        if self.in_tail_position {
                            Ok(Expr::TailCall {
                                func: func_idx,
                                args: vec![],
                            })
                        } else {
                            Ok(Expr::Call {
                                func: func_idx,
                                args: vec![],
                            })
                        }
                    } else {
                        // Non-zero-arg functions: wrap as closure
                        self.lower_user_func_as_closure(&resolved_name, func_idx, arity)
                    }
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
                } else if let Edn::Keyword(kw) = &items[0] {
                    // Keyword in call position: desugar to get
                    // (:foo map) -> (get map :foo)
                    // Note: 3-arg form (:foo map default) is not yet supported
                    // because get doesn't handle default values
                    if items.len() != 2 {
                        return Err(CompileError::Unsupported(
                            "Keyword in call position requires exactly 1 argument (the map)".into(),
                        ));
                    }
                    let get_call = Edn::List(vec![
                        Edn::Symbol(Symbol::new("get")),
                        items[1].clone(),
                        Edn::Keyword(kw.clone()),
                    ]);
                    self.lower_expr(&get_call)
                } else {
                    // Expression in call position - treat as closure call
                    // Examples: ((fn [x] x) 5), ((if cond + -) a b)
                    // Variadic closures are handled by generate_closure_call detecting
                    // VARIADIC_CAPTURE type_id and packing args appropriately.

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
                    // Empty vector is an irreducible primitive (used by core.sus vector fn)
                    Ok(Expr::VecNew(vec![]))
                } else {
                    // Desugar [1 2 3] -> (vector 1 2 3)
                    let mut call = vec![Edn::Symbol(Symbol::new("vector"))];
                    call.extend(items.iter().cloned());
                    self.lower_expr(&Edn::List(call))
                }
            }

            Edn::Map(pairs) => {
                if pairs.is_empty() {
                    // Empty map is an irreducible primitive (used by core.sus hash-map fn)
                    Ok(Expr::MapNew(vec![]))
                } else {
                    // Desugar {k1 v1 k2 v2} -> (hash-map k1 v1 k2 v2)
                    let mut call = vec![Edn::Symbol(Symbol::new("hash-map"))];
                    for (k, v) in pairs {
                        call.push(k.clone());
                        call.push(v.clone());
                    }
                    self.lower_expr(&Edn::List(call))
                }
            }

            Edn::Set(items) => {
                if items.is_empty() {
                    // Empty set is an irreducible primitive (used by core.sus hash-set fn)
                    Ok(Expr::SetNew(vec![]))
                } else {
                    // Desugar #{1 2 3} -> (hash-set 1 2 3)
                    let mut call = vec![Edn::Symbol(Symbol::new("hash-set"))];
                    call.extend(items.iter().cloned());
                    self.lower_expr(&Edn::List(call))
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
            // Special form: call a variadic function directly with an args array
            // Used by variadic function wrappers to avoid double-packing
            // ($direct-variadic-call func-name args-array)
            "$direct-variadic-call" => {
                if args.len() != 2 {
                    return Err(CompileError::Parse(
                        "$direct-variadic-call requires 2 arguments: func-name and args-array".into(),
                    ));
                }
                let func_name = match &args[0] {
                    Edn::Symbol(sym) => sym.name.clone(),
                    _ => return Err(CompileError::Parse(
                        "$direct-variadic-call first arg must be a symbol".into(),
                    )),
                };
                if let Some((_, idx)) = self.resolve_func_name(&func_name) {
                    let args_array = self.lower_expr(&args[1])?;
                    // Emit direct call without variadic packing
                    Ok(Expr::Call {
                        func: idx,
                        args: vec![args_array],
                    })
                } else {
                    Err(CompileError::Undefined(func_name))
                }
            }

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
            "rem" => {
                let ty = self.infer_numeric_type(args);
                self.lower_binop(BinOp::Rem, args, ty)
            }
            "quot" => {
                let ty = self.infer_numeric_type(args);
                self.lower_binop(BinOp::Quot, args, ty)
            }
            // NOTE: "mod" is now a regular function in core.sus (floored modulus)

            // Numeric operations (desugar to primitives)
            "inc" => self.lower_inc(args),
            "dec" => self.lower_dec(args),
            "abs" => self.lower_abs(args),
            "min" => self.lower_min(args),
            "max" => self.lower_max(args),

            // Comparison - use suss-equals for structural equality
            // Supports variadic: (= a b c) → (and (= a b) (= b c))
            "=" => {
                if args.len() < 2 {
                    return Err(CompileError::Parse(
                        "= requires at least 2 arguments".into(),
                    ));
                }
                // Call suss-equals function for proper structural comparison
                if let Some((_, idx)) = self.resolve_func_name("suss-equals") {
                    if args.len() == 2 {
                        // Simple 2-arg case
                        let lowered_args = args
                            .iter()
                            .map(|arg| self.lower_expr(arg))
                            .collect::<Result<Vec<_>, _>>()?;
                        Ok(Expr::Call {
                            func: idx,
                            args: lowered_args,
                        })
                    } else {
                        // Variadic: (= a b c) → (and (suss-equals a b) (suss-equals b c))
                        self.with_tail_disabled(|l| {
                            let mut comparisons = Vec::new();
                            for pair in args.windows(2) {
                                let left = l.lower_expr(&pair[0])?;
                                let right = l.lower_expr(&pair[1])?;
                                comparisons.push(Expr::Call {
                                    func: idx,
                                    args: vec![left, right],
                                });
                            }
                            // Chain with AND using short-circuit if/else
                            let mut result = comparisons.pop().unwrap();
                            while let Some(cmp) = comparisons.pop() {
                                result = Expr::If {
                                    cond: Box::new(cmp),
                                    then_branch: Box::new(result),
                                    else_branch: Box::new(Expr::Bool(false)),
                                    ty: Type::Bool,
                                };
                            }
                            Ok(result)
                        })
                    }
                } else {
                    // Fallback to primitive comparison if suss-equals not available
                    self.lower_comparison_chain(BinOp::Eq, args)
                }
            }
            // not= is now handled by core.sus as (defn not= [& args] (not (apply = args)))
            "<" => self.lower_comparison_chain(BinOp::Lt, args),
            "<=" => self.lower_comparison_chain(BinOp::Le, args),
            ">" => self.lower_comparison_chain(BinOp::Gt, args),
            ">=" => self.lower_comparison_chain(BinOp::Ge, args),

            // Logical
            "not" => {
                if args.len() != 1 {
                    return Err(CompileError::Parse("not requires exactly 1 argument".into()));
                }
                // Operand is never in tail position - we need to test its value
                let operand = self.with_tail_disabled(|l| l.lower_expr(&args[0]))?;
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
                // Operand is never in tail position - we need to test its value
                let operand = self.with_tail_disabled(|l| l.lower_expr(&args[0]))?;
                // nil? returns true if operand equals nil sentinel (i31ref(0))
                Ok(Expr::NilCheck(Box::new(operand)))
            }

            // identical? - reference equality check using WASM ref.eq
            "identical?" => {
                if args.len() != 2 {
                    return Err(CompileError::Parse(
                        "identical? requires exactly 2 arguments".into(),
                    ));
                }
                // Arguments to identical? are never in tail position
                let (left, right) = self.with_tail_disabled(|l| -> CompileResult<_> {
                    Ok((l.lower_expr(&args[0])?, l.lower_expr(&args[1])?))
                })?;
                Ok(Expr::Identical(Box::new(left), Box::new(right)))
            }

            // prim-eq - primitive equality using polymorphic unwrap
            // Handles keywords/symbols by comparing their name_idx, not references
            "prim-eq" => {
                if args.len() != 2 {
                    return Err(CompileError::Parse(
                        "prim-eq requires exactly 2 arguments".into(),
                    ));
                }
                self.lower_binop(BinOp::Eq, args, Type::Bool)
            }

            // Quote - create first-class symbols and quoted data structures
            "quote" => {
                if args.len() != 1 {
                    return Err(CompileError::Parse("quote requires exactly 1 argument".into()));
                }
                self.lower_quoted(&args[0])
            }

            // Var - get the Var object for a symbol
            // #'x or (var x) returns the Var containing x's value
            "var" => {
                if args.len() != 1 {
                    return Err(CompileError::Parse("var requires exactly 1 argument".into()));
                }
                self.lower_var(&args[0])
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
            // NOTE: "conj" is now a variadic function in core.sus
            // NOTE: "cons" is now a regular function in core.sus, not a compiler builtin
            "count" => self.lower_count(args),
            "get" => self.lower_get(args),
            // NOTE: "assoc" is now a variadic function in core.sus
            // NOTE: "dissoc" is now a variadic function in core.sus
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
            "make-string" => self.lower_make_string(args),
            "scopy" => self.lower_scopy(args),
            "sget" => self.lower_sget(args),
            "sset" => self.lower_sset(args),

            // Type checking
            "instance?" => self.lower_instance_check(args),
            "symbol?" => self.lower_symbol_check(args),
            "keyword?" => self.lower_keyword_check(args),
            "var?" => self.lower_var_check(args),
            "int?" => self.lower_int_check(args),
            "float?" => self.lower_float_check(args),
            "string?" => self.lower_string_check(args),

            // Float operations
            "trunc" => self.lower_f64_trunc(args),
            "f64->i64" => self.lower_f64_to_i64(args),
            "i64->f64" => self.lower_i64_to_f64(args),
            "sqrt" => self.lower_f64_sqrt(args),
            "Math/sqrt" => self.lower_f64_sqrt(args),
            "int" => self.lower_f64_to_i64(args), // int is alias for f64->i64

            // Numeric predicates
            "zero?" => self.lower_zero_check(args),
            "pos?" => self.lower_pos_check(args),
            "neg?" => self.lower_neg_check(args),
            "even?" => self.lower_even_check(args),
            "odd?" => self.lower_odd_check(args),

            // Symbol/keyword/var introspection
            "name" => self.lower_name(args),
            "namespace" => self.lower_namespace(args),
            "symbol" => self.lower_symbol_constructor(args),
            "meta" => self.lower_meta(args),

            // Mutable field set: (set! (.-field obj) value)
            "set!" => self.lower_set(args),

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

        // Check user-defined types first (for_mutation=false: reading field)
        if let Some((type_idx, field_idx, field_type)) = self.lookup_user_field(field_name, false) {
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
    /// defined in core.sus, so all deftypes are handled uniformly here.
    ///
    /// If `current_self_type` is set (during protocol method lowering), prioritize
    /// looking up fields in that type to handle fields with the same name in
    /// different types (e.g., "root" exists in both PersistentVector and PersistentMap).
    /// Look up a field by name across user-defined types.
    /// When `for_mutation` is true, only returns mutable fields (for set! operations).
    fn lookup_user_field(&self, field_name: &str, for_mutation: bool) -> Option<(u32, u32, FieldType)> {
        // If we're in a protocol method implementation, check the self type first
        if let Some(ref self_type) = self.current_self_type {
            if let Some(info) = self.user_types.get(self_type.as_str()) {
                for (idx, field) in info.fields.iter().enumerate() {
                    if field.name == field_name && (!for_mutation || field.is_mutable) {
                        // Field 0 is type_id, so user fields start at index 1
                        return Some((info.gc_type_idx, (idx + 1) as u32, field.field_type));
                    }
                }
            }
        }

        // Fall back to searching all user types
        for info in self.user_types.values() {
            for (idx, field) in info.fields.iter().enumerate() {
                if field.name == field_name && (!for_mutation || field.is_mutable) {
                    // Field 0 is type_id, so user fields start at index 1
                    return Some((info.gc_type_idx, (idx + 1) as u32, field.field_type));
                }
            }
        }
        None
    }

    /// Lower set!: (set! (.-field obj) value)
    /// Only works with mutable fields on user-defined types.
    fn lower_set(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 2 {
            return Err(CompileError::Parse(format!(
                "set! requires exactly 2 arguments, got {}",
                args.len()
            )));
        }

        // First arg must be a field access: (.-field obj)
        let field_access = &args[0];
        let value = &args[1];

        match field_access {
            Edn::List(items) if items.len() == 2 => {
                // Check it's a field access (.-field obj)
                if let Edn::Symbol(sym) = &items[0] {
                    if sym.name.starts_with(".-") {
                        let field_name = &sym.name[2..]; // Strip ".-" prefix

                        // Look up the field in user types (for_mutation=true: only mutable fields)
                        if let Some((type_idx, field_idx, field_type)) = self.lookup_user_field(field_name, true) {
                            // Lower the object and value expressions
                            let obj = self.with_tail_disabled(|l| l.lower_expr(&items[1]))?;
                            let val = self.with_tail_disabled(|l| l.lower_expr(value))?;

                            return Ok(Expr::StructSet {
                                type_idx,
                                field_idx,
                                field_type,
                                obj: Box::new(obj),
                                value: Box::new(val),
                            });
                        } else {
                            return Err(CompileError::Undefined(format!(
                                "set!: unknown or immutable field {}",
                                field_name
                            )));
                        }
                    }
                }
                Err(CompileError::Parse(
                    "set! first argument must be a field access (.-field obj)".into()
                ))
            }
            _ => Err(CompileError::Parse(
                "set! first argument must be a field access (.-field obj)".into()
            )),
        }
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

        // Check user-defined types first (includes collection types from core.sus)
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

    fn lower_symbol_check(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 1 {
            return Err(CompileError::Parse("symbol? requires exactly 1 argument".into()));
        }
        let obj = self.with_tail_disabled(|l| l.lower_expr(&args[0]))?;
        Ok(Expr::RefTest {
            type_idx: gc_types::SYMBOL,
            value: Box::new(obj),
        })
    }

    fn lower_keyword_check(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 1 {
            return Err(CompileError::Parse("keyword? requires exactly 1 argument".into()));
        }
        let obj = self.with_tail_disabled(|l| l.lower_expr(&args[0]))?;
        Ok(Expr::RefTest {
            type_idx: gc_types::KEYWORD,
            value: Box::new(obj),
        })
    }

    fn lower_var_check(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 1 {
            return Err(CompileError::Parse("var? requires exactly 1 argument".into()));
        }
        let obj = self.with_tail_disabled(|l| l.lower_expr(&args[0]))?;
        Ok(Expr::RefTest {
            type_idx: gc_types::VAR,
            value: Box::new(obj),
        })
    }

    fn lower_name(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 1 {
            return Err(CompileError::Parse("name requires exactly 1 argument".into()));
        }
        let obj = self.with_tail_disabled(|l| l.lower_expr(&args[0]))?;
        // Get the name field from symbol or keyword
        // For symbols: name_str_idx is at field 3
        // For keywords: name_idx is at field 2 (but it's a keyword table index, not string index)
        // We'll use a runtime dispatch in codegen to handle both
        Ok(Expr::GetName(Box::new(obj)))
    }

    fn lower_namespace(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 1 {
            return Err(CompileError::Parse("namespace requires exactly 1 argument".into()));
        }
        let obj = self.with_tail_disabled(|l| l.lower_expr(&args[0]))?;
        // Get the namespace field from symbol or keyword
        // For symbols: ns_str_idx is at field 2 (-1 if no namespace)
        // For keywords: namespace is part of the keyword table entry
        Ok(Expr::GetNamespace(Box::new(obj)))
    }

    fn lower_meta(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 1 {
            return Err(CompileError::Parse("meta requires exactly 1 argument".into()));
        }
        let obj = self.with_tail_disabled(|l| l.lower_expr(&args[0]))?;
        // Get the metadata field from a Var
        Ok(Expr::VarMeta(Box::new(obj)))
    }

    fn lower_symbol_constructor(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        // (symbol name) or (symbol ns name)
        match args.len() {
            1 => {
                // (symbol name) - create symbol from string
                let name_expr = self.with_tail_disabled(|l| l.lower_expr(&args[0]))?;
                Ok(Expr::SymbolFromString {
                    ns: None,
                    name: Box::new(name_expr),
                })
            }
            2 => {
                // (symbol ns name) - create namespaced symbol
                let ns_expr = self.with_tail_disabled(|l| l.lower_expr(&args[0]))?;
                let name_expr = self.with_tail_disabled(|l| l.lower_expr(&args[1]))?;
                Ok(Expr::SymbolFromString {
                    ns: Some(Box::new(ns_expr)),
                    name: Box::new(name_expr),
                })
            }
            _ => Err(CompileError::Parse("symbol requires 1 or 2 arguments".into())),
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

    /// Lower a variadic comparison operator like (< a b c) into chained comparisons
    /// Result: (and (< a b) (< b c))
    /// Note: This re-evaluates intermediate arguments, which is fine for simple expressions.
    fn lower_comparison_chain(&mut self, op: BinOp, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() < 2 {
            return Err(CompileError::Parse(format!(
                "Comparison operator requires at least 2 arguments, got {}",
                args.len()
            )));
        }

        // For exactly 2 args, just do simple comparison
        if args.len() == 2 {
            return self.lower_binop(op, args, Type::Bool);
        }

        // For 3+ args, chain comparisons with AND
        // (< a b c) → (and (< a b) (< b c))
        self.with_tail_disabled(|l| {
            let mut comparisons = Vec::new();
            for pair in args.windows(2) {
                let left = l.lower_expr(&pair[0])?;
                let right = l.lower_expr(&pair[1])?;
                comparisons.push(Expr::BinOp {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                    ty: Type::Bool,
                });
            }

            // Chain with AND
            let mut result = comparisons.pop().unwrap();
            while let Some(cmp) = comparisons.pop() {
                result = Expr::If {
                    cond: Box::new(cmp),
                    then_branch: Box::new(result),
                    else_branch: Box::new(Expr::Bool(false)),
                    ty: Type::Bool,
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

    /// Lower a quoted expression to create first-class data.
    /// Quote prevents evaluation and creates data values:
    /// - 'sym → SYMBOL struct
    /// - :kw → KEYWORD struct (keywords evaluate to themselves)
    /// - '[...] → vector with quoted elements
    /// - literals → themselves
    fn lower_quoted(&mut self, edn: &Edn) -> CompileResult<Expr> {
        use crate::ir::gc_types;

        match edn {
            // Quoted symbol → first-class SYMBOL struct
            Edn::Symbol(sym) => {
                let namespace = sym.namespace.as_deref();
                let name = &sym.name;

                // Compute hash for map key operations
                let hash = gc_types::hash_symbol(namespace, name);

                // Intern into symbol table (like keywords use intern_keyword)
                let symbol_idx = self.module.intern_symbol(namespace, name);

                Ok(Expr::Symbol {
                    hash,
                    ns_str_idx: -1,              // No longer needed for lookup
                    name_str_idx: symbol_idx,   // Now a symbol table index
                })
            }

            // Keywords evaluate to themselves, so quoting them is the same
            Edn::Keyword(kw) => {
                let namespace = kw.namespace.as_deref();
                let name = &kw.name;
                let hash = gc_types::hash_keyword(namespace, name);
                let idx = self.module.intern_keyword(namespace, name);
                Ok(Expr::Keyword { idx, hash })
            }

            // Quoted vector → vector with quoted elements
            Edn::Vector(elems) => {
                let mut quoted_elems = Vec::with_capacity(elems.len());
                for elem in elems {
                    quoted_elems.push(self.lower_quoted(elem)?);
                }
                Ok(Expr::VecNew(quoted_elems))
            }

            // Quoted list → list with quoted elements (as cons cells)
            // For now, treat as vector since we don't have first-class lists yet
            Edn::List(elems) => {
                let mut quoted_elems = Vec::with_capacity(elems.len());
                for elem in elems {
                    quoted_elems.push(self.lower_quoted(elem)?);
                }
                Ok(Expr::VecNew(quoted_elems))
            }

            // Quoted map → map with quoted key-value pairs
            Edn::Map(pairs) => {
                let mut quoted_pairs = Vec::with_capacity(pairs.len());
                for (k, v) in pairs {
                    let qk = self.lower_quoted(k)?;
                    let qv = self.lower_quoted(v)?;
                    quoted_pairs.push((qk, qv));
                }
                Ok(Expr::MapNew(quoted_pairs))
            }

            // Quoted set → set with quoted elements
            Edn::Set(elems) => {
                let mut quoted_elems = Vec::with_capacity(elems.len());
                for elem in elems {
                    quoted_elems.push(self.lower_quoted(elem)?);
                }
                Ok(Expr::SetNew(quoted_elems))
            }

            // Literals evaluate to themselves
            Edn::Nil => Ok(Expr::Unit),
            Edn::Bool(b) => Ok(Expr::Bool(*b)),
            Edn::Number(n) => {
                // Convert number to IR expression
                match n {
                    suss_core::Number::Integer(i) => {
                        // Try to convert to i64
                        if let Some(val) = i.to_i64() {
                            Ok(Expr::Int(val))
                        } else {
                            Err(CompileError::Unsupported("BigInt not supported yet".into()))
                        }
                    }
                    suss_core::Number::Float(f) => Ok(Expr::Float(*f)),
                    suss_core::Number::Ratio(_) => {
                        Err(CompileError::Unsupported("Ratio not supported yet".into()))
                    }
                }
            }
            Edn::String(s) => {
                let idx = self.module.intern_string(s);
                Ok(Expr::String(idx))
            }
            Edn::Char(c) => {
                // Characters are represented as their Unicode code point
                Ok(Expr::Int(*c as i64))
            }

            // Tagged literals - for now, just return the value
            Edn::Tagged(tagged) => self.lower_quoted(&tagged.value),

            // Reader conditionals should be resolved before lowering
            Edn::ReaderConditional(_) => {
                Err(CompileError::Unsupported("Reader conditional in quote".into()))
            }

            // Runtime-only values cannot be quoted
            Edn::Primitive(_) | Edn::Function { .. } => {
                Err(CompileError::Unsupported("Cannot quote runtime value".into()))
            }
        }
    }

    /// Lower (var x) or #'x - get the Var for a symbol
    ///
    /// Currently creates a Var on the fly since def doesn't create Vars yet.
    /// When def is updated to create Vars, this will look up the existing Var.
    fn lower_var(&mut self, arg: &Edn) -> CompileResult<Expr> {
        use crate::ir::gc_types;

        // arg should be a symbol
        let Edn::Symbol(sym) = arg else {
            return Err(CompileError::Parse(format!(
                "var requires a symbol, got {:?}",
                arg
            )));
        };

        let name = &sym.name;
        let namespace = sym.namespace.as_deref();

        // Try to get the value for this symbol
        let value = if let Some(&(idx, ref ty)) = self.locals.get(name) {
            // It's a local variable
            Expr::LocalGet { local: idx, ty: ty.clone() }
        } else if let Some(&func_idx) = self.func_indices.get(name) {
            // User-defined function - create a closure wrapper
            let arity = self.func_arities.get(name).copied().unwrap_or(0);
            if self.variadic_funcs.contains_key(name) {
                // Variadic user function - create variadic closure
                self.lower_variadic_user_func_as_closure(name, func_idx)?
            } else {
                // Regular user function
                self.lower_user_func_as_closure(name, func_idx, arity)?
            }
        } else if let Some(arity) = self.builtin_arity(name) {
            // Built-in function
            self.lower_builtin_as_closure(name, arity)?
        } else {
            return Err(CompileError::Undefined(name.clone()));
        };

        // Create a SYMBOL struct for the symbol name
        let hash = gc_types::hash_symbol(namespace, name);
        let ns_str_idx = if let Some(ns) = namespace {
            self.module.intern_string(ns) as i32
        } else {
            -1
        };
        let name_str_idx = self.module.intern_string(name);
        let sym_expr = Expr::Symbol {
            hash,
            ns_str_idx,
            name_str_idx,
        };

        // Create a Var: { root: value, meta: nil, sym: symbol }
        Ok(Expr::VarNew {
            root: Box::new(value),
            meta: Box::new(Expr::Unit), // nil metadata
            sym: Box::new(sym_expr),
        })
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
    /// Single-arity: (fn [params] body) → ClosureNew { func_idx, arity, captures }
    /// Multi-arity: (fn ([p1] b1) ([p2] b2) ...) → variadic closure with dispatch
    fn lower_fn(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.is_empty() {
            return Err(CompileError::Parse("fn requires params vector".into()));
        }

        // Check for multi-arity: (fn ([p1] b1) ([p2] b2) ...)
        // Each clause is a List starting with a Vector
        if let Edn::List(clause) = &args[0] {
            if matches!(clause.first(), Some(Edn::Vector(_))) {
                return self.lower_multi_arity_fn(args);
            }
        }

        // Parse params vector (single-arity)
        let params_vec = match &args[0] {
            Edn::Vector(items) => items,
            _ => return Err(CompileError::Parse("fn params must be a vector".into())),
        };

        let mut params = Vec::new();
        let mut rest_param: Option<String> = None;
        let mut found_amp = false;
        for param in params_vec {
            match param {
                Edn::Symbol(sym) if sym.name == "&" => {
                    found_amp = true;
                }
                Edn::Symbol(sym) if found_amp => {
                    rest_param = Some(sym.name.clone());
                    // No more params after rest
                    break;
                }
                Edn::Symbol(sym) => params.push(sym.name.clone()),
                _ => return Err(CompileError::Parse("fn param must be a symbol".into())),
            }
        }
        let is_variadic = rest_param.is_some();

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

        // Calculate wrapper function index in IR
        // Closures are added after deftype constructors, deftype impls, analyzed functions, and extension methods
        // Use closure_counter (not pending_closures.len()) because pending closures may have been
        // taken out during wrapper generation but their slots are still occupied
        let closure_num = self.closure_counter;
        let wrapper_idx = self.num_deftype_constructors + self.num_deftype_impl_funcs + self.num_analyzed_funcs + self.num_extension_funcs + closure_num;

        // Generate unique wrapper function name
        let wrapper_name = format!("$closure_{}", closure_num);
        self.closure_counter += 1;

        // Register wrapper function name for the index
        self.func_indices.insert(wrapper_name.clone(), wrapper_idx);

        // Store closure wrapper for later generation
        self.pending_closures.push(ClosureWrapper {
            name: wrapper_name,
            captures: free_vars.clone(),
            params,
            rest_param: rest_param.clone(),
            body,
            is_variadic,
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

        // For variadic closures, arity is 1 (the args_array parameter)
        // For regular closures, arity is the number of fixed params
        let arity = if is_variadic {
            1
        } else {
            self.pending_closures.last().unwrap().params.len() as u32
        };

        Ok(Expr::ClosureNew {
            func_idx: wrapper_idx,
            arity,
            captures,
            is_variadic,
        })
    }

    /// Lower multi-arity fn to a variadic closure with dispatch
    /// (fn ([x] x) ([x y] (+ x y))) becomes:
    /// (fn [& args]
    ///   (case (count args)
    ///     1 (let [x (nth args 0)] x)
    ///     2 (let [x (nth args 0) y (nth args 1)] (+ x y))
    ///     default-error))
    fn lower_multi_arity_fn(&mut self, clauses: &[Edn]) -> CompileResult<Expr> {
        use suss_core::Symbol;

        // Parse each arity clause
        struct ArityClause {
            arity: usize,
            params: Vec<String>,
            rest_param: Option<String>,
            body: Vec<Edn>,
        }

        let mut parsed_clauses = Vec::new();
        let mut has_variadic = false;

        for clause in clauses {
            let clause_items = match clause {
                Edn::List(items) => items,
                _ => return Err(CompileError::Parse("Multi-arity clause must be a list".into())),
            };

            if clause_items.is_empty() {
                return Err(CompileError::Parse("Empty arity clause".into()));
            }

            let params_vec = match &clause_items[0] {
                Edn::Vector(items) => items,
                _ => return Err(CompileError::Parse("Arity clause must start with params vector".into())),
            };

            let mut params = Vec::new();
            let mut rest_param: Option<String> = None;
            let mut found_amp = false;
            for param in params_vec {
                match param {
                    Edn::Symbol(sym) if sym.name == "&" => {
                        found_amp = true;
                    }
                    Edn::Symbol(sym) if found_amp => {
                        rest_param = Some(sym.name.clone());
                        has_variadic = true;
                        break;
                    }
                    Edn::Symbol(sym) => params.push(sym.name.clone()),
                    _ => return Err(CompileError::Parse("Param must be a symbol".into())),
                }
            }

            let arity = params.len();
            let body: Vec<Edn> = clause_items[1..].to_vec();

            parsed_clauses.push(ArityClause {
                arity,
                params,
                rest_param,
                body,
            });
        }

        // Sort by arity (variadic clause last)
        parsed_clauses.sort_by(|a, b| {
            match (&a.rest_param, &b.rest_param) {
                (Some(_), None) => std::cmp::Ordering::Greater,
                (None, Some(_)) => std::cmp::Ordering::Less,
                _ => a.arity.cmp(&b.arity),
            }
        });

        // Build the dispatch body using nested if on (count args)
        // (if (= (count args) N1) body1 (if (= (count args) N2) body2 default))
        let args_sym = Symbol::new("$multi_arity_args");

        // Helper to build a clause body with let bindings
        // Note: args is a raw WASM array, so we use aget/alength instead of nth/count
        let build_clause_body = |clause: &ArityClause| -> Edn {
            let mut let_bindings = Vec::new();
            for (i, param) in clause.params.iter().enumerate() {
                let_bindings.push(Edn::Symbol(Symbol::new(param)));
                let_bindings.push(Edn::List(vec![
                    Edn::Symbol(Symbol::new("aget")),
                    Edn::Symbol(args_sym.clone()),
                    Edn::Number(suss_core::Number::Integer((i as i64).into())),
                ]));
            }
            // Add rest param binding if present
            // For now, we create a lazy-seq that iterates over the remaining array elements
            if let Some(ref rest_name) = clause.rest_param {
                let_bindings.push(Edn::Symbol(Symbol::new(rest_name)));
                // TODO: properly implement rest args for multi-arity
                // For now, just use nil as placeholder
                let_bindings.push(Edn::Nil);
            }

            let let_body = if clause.body.len() == 1 {
                clause.body[0].clone()
            } else {
                let mut do_form = vec![Edn::Symbol(Symbol::new("do"))];
                do_form.extend(clause.body.clone());
                Edn::List(do_form)
            };

            if let_bindings.is_empty() {
                // 0-arity: no bindings needed
                let_body
            } else {
                Edn::List(vec![
                    Edn::Symbol(Symbol::new("let")),
                    Edn::Vector(let_bindings),
                    let_body,
                ])
            }
        };

        // Build nested if from back to front
        let mut dispatch_expr = if has_variadic {
            // Default is the variadic clause
            build_clause_body(parsed_clauses.last().unwrap())
        } else {
            Edn::Nil
        };

        // Process fixed-arity clauses in reverse order (excluding variadic if present)
        let fixed_clauses: Vec<_> = parsed_clauses.iter()
            .filter(|c| c.rest_param.is_none())
            .collect();

        for clause in fixed_clauses.into_iter().rev() {
            let condition = Edn::List(vec![
                Edn::Symbol(Symbol::new("=")),
                Edn::List(vec![
                    Edn::Symbol(Symbol::new("alength")),
                    Edn::Symbol(args_sym.clone()),
                ]),
                Edn::Number(suss_core::Number::Integer((clause.arity as i64).into())),
            ]);

            let then_body = build_clause_body(clause);

            dispatch_expr = Edn::List(vec![
                Edn::Symbol(Symbol::new("if")),
                condition,
                then_body,
                dispatch_expr,
            ]);
        }

        // Now lower as a regular variadic fn: (fn [& args] dispatch-body)
        let variadic_fn = vec![
            Edn::Vector(vec![
                Edn::Symbol(Symbol::new("&")),
                Edn::Symbol(args_sym),
            ]),
            dispatch_expr,
        ];

        self.lower_fn(&variadic_fn)
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

    /// (zero? n) -> (= n 0)
    fn lower_zero_check(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 1 {
            return Err(CompileError::Parse("zero? requires exactly 1 argument".into()));
        }
        let n = self.with_tail_disabled(|l| l.lower_expr(&args[0]))?;
        Ok(Expr::BinOp {
            op: BinOp::Eq,
            left: Box::new(n),
            right: Box::new(Expr::Int(0)),
            ty: Type::Bool,
        })
    }

    /// (pos? n) -> (> n 0)
    fn lower_pos_check(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 1 {
            return Err(CompileError::Parse("pos? requires exactly 1 argument".into()));
        }
        let n = self.with_tail_disabled(|l| l.lower_expr(&args[0]))?;
        Ok(Expr::BinOp {
            op: BinOp::Gt,
            left: Box::new(n),
            right: Box::new(Expr::Int(0)),
            ty: Type::Bool,
        })
    }

    /// (neg? n) -> (< n 0)
    fn lower_neg_check(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 1 {
            return Err(CompileError::Parse("neg? requires exactly 1 argument".into()));
        }
        let n = self.with_tail_disabled(|l| l.lower_expr(&args[0]))?;
        Ok(Expr::BinOp {
            op: BinOp::Lt,
            left: Box::new(n),
            right: Box::new(Expr::Int(0)),
            ty: Type::Bool,
        })
    }

    /// (even? n) -> (= 0 (mod n 2))
    fn lower_even_check(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 1 {
            return Err(CompileError::Parse("even? requires exactly 1 argument".into()));
        }
        let n = self.with_tail_disabled(|l| l.lower_expr(&args[0]))?;
        let mod_result = Expr::BinOp {
            op: BinOp::Rem,
            left: Box::new(n),
            right: Box::new(Expr::Int(2)),
            ty: Type::I32,
        };
        Ok(Expr::BinOp {
            op: BinOp::Eq,
            left: Box::new(mod_result),
            right: Box::new(Expr::Int(0)),
            ty: Type::Bool,
        })
    }

    /// (odd? n) -> (not= 0 (mod n 2))
    fn lower_odd_check(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 1 {
            return Err(CompileError::Parse("odd? requires exactly 1 argument".into()));
        }
        let n = self.with_tail_disabled(|l| l.lower_expr(&args[0]))?;
        let mod_result = Expr::BinOp {
            op: BinOp::Rem,
            left: Box::new(n),
            right: Box::new(Expr::Int(2)),
            ty: Type::I32,
        };
        Ok(Expr::BinOp {
            op: BinOp::Ne,
            left: Box::new(mod_result),
            right: Box::new(Expr::Int(0)),
            ty: Type::Bool,
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
        // Check if it's a qualified call (e.g., "random/get-random-u64", "utils/helper", "myapp.core/main")
        if let Some(slash_pos) = name.find('/') {
            let prefix = &name[..slash_pos];
            let func_name = &name[slash_pos + 1..];

            // 1. Check WASI/WIT imports first
            if let Some(&idx) = self.import_indices.get(&(prefix.to_string(), func_name.to_string())) {
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

            // 2. Check if prefix is a namespace alias -> resolve to full namespace
            let target_ns = self.ns_aliases.get(prefix).cloned();
            let qualified_name = if let Some(ns) = target_ns {
                format!("{}/{}", ns, func_name)
            } else {
                // Prefix might be a full namespace name already (e.g., "myapp.core/main")
                name.to_string()
            };

            // 3. Look up the qualified name in func_indices
            if let Some(&idx) = self.func_indices.get(&qualified_name) {
                return self.emit_func_call(idx, &qualified_name, args);
            }

            // Not found - provide helpful error
            if prefix.starts_with("wasi.") || prefix.contains(':') {
                return Err(CompileError::Undefined(format!(
                    "WASI function '{}' not found. Make sure the interface is imported.",
                    name
                )));
            } else {
                return Err(CompileError::Undefined(format!(
                    "Function '{}' not found (namespace: '{}', function: '{}')",
                    name, prefix, func_name
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

        // Resolve function name through namespace resolution chain
        if let Some((resolved_name, idx)) = self.resolve_func_name(name) {
            return self.emit_func_call(idx, &resolved_name, args);
        }

        // Check if it's a local variable (could be a closure)
        if let Some(&(local_idx, ref ty)) = self.locals.get(name) {
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

    /// Lower (nth coll index) or (nth coll index default) -> VecNth or ProtocolDispatch
    ///
    /// If the collection type is known at compile time, uses the fast path.
    /// Otherwise, falls back to runtime protocol dispatch.
    /// With 3 args, returns default if index is out of bounds.
    fn lower_nth(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        use crate::ir::{gc_types, method_ids};

        if args.len() < 2 || args.len() > 3 {
            return Err(CompileError::Parse(
                "nth requires 2 or 3 arguments: collection, index, [default]".into(),
            ));
        }

        let has_default = args.len() == 3;

        // Arguments are never in tail position
        let coll = self.with_tail_disabled(|l| l.lower_expr(&args[0]))?;
        let index = self.with_tail_disabled(|l| l.lower_expr(&args[1]))?;
        let default = if has_default {
            Some(self.with_tail_disabled(|l| l.lower_expr(&args[2]))?)
        } else {
            None
        };

        // Build the nth operation
        let nth_expr = if let Some(type_id) = self.infer_collection_type(&args[0]) {
            if type_id == gc_types::PERSISTENT_VECTOR {
                Expr::VecNth {
                    vec: Box::new(coll),
                    index: Box::new(index),
                }
            } else {
                Expr::ProtocolDispatch {
                    obj: Box::new(coll),
                    method_id: method_ids::NTH,
                    args: vec![index],
                    in_tail_position: false,
                }
            }
        } else {
            Expr::ProtocolDispatch {
                obj: Box::new(coll),
                method_id: method_ids::NTH,
                args: vec![index],
                in_tail_position: false,
            }
        };

        // If we have a default, wrap in: (let [result nth-expr] (if (nil? result) default result))
        if let Some(default_val) = default {
            let result_local = self.next_local;
            self.next_local += 1;
            Ok(Expr::Let {
                bindings: vec![(result_local, nth_expr)],
                body: Box::new(Expr::If {
                    cond: Box::new(Expr::NilCheck(Box::new(Expr::LocalGet {
                        local: result_local,
                        ty: Type::GcRef,
                    }))),
                    then_branch: Box::new(default_val),
                    else_branch: Box::new(Expr::LocalGet {
                        local: result_local,
                        ty: Type::GcRef,
                    }),
                    ty: Type::GcRef,
                }),
            })
        } else {
            Ok(nth_expr)
        }
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
    /// Vector conj now uses protocol dispatch (implemented in core.sus).
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
                // Vector conj now uses protocol dispatch (core.sus implementation)
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

    // NOTE: lower_cons() removed - cons is now a regular function in core.sus
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
    /// With 3 args, returns default if key is not found.
    fn lower_get(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        use crate::ir::{gc_types, method_ids};

        if args.len() < 2 || args.len() > 3 {
            return Err(CompileError::Parse(
                "get requires 2 or 3 arguments: collection, key, [default]".into(),
            ));
        }

        let has_default = args.len() == 3;

        // Arguments are never in tail position
        let coll = self.with_tail_disabled(|l| l.lower_expr(&args[0]))?;
        let key = self.with_tail_disabled(|l| l.lower_expr(&args[1]))?;
        let default = if has_default {
            Some(self.with_tail_disabled(|l| l.lower_expr(&args[2]))?)
        } else {
            None
        };

        // Build the get operation
        let get_expr = if let Some(type_id) = self.infer_collection_type(&args[0]) {
            match type_id {
                t if t == gc_types::PERSISTENT_MAP => {
                    Expr::ProtocolDispatch {
                        obj: Box::new(coll),
                        method_id: method_ids::LOOKUP,
                        args: vec![key],
                        in_tail_position: false,
                    }
                }
                t if t == gc_types::PERSISTENT_VECTOR => {
                    Expr::VecNth {
                        vec: Box::new(coll),
                        index: Box::new(key),
                    }
                }
                _ => Expr::ProtocolDispatch {
                    obj: Box::new(coll),
                    method_id: method_ids::LOOKUP,
                    args: vec![key],
                    in_tail_position: false,
                },
            }
        } else {
            Expr::ProtocolDispatch {
                obj: Box::new(coll),
                method_id: method_ids::LOOKUP,
                args: vec![key],
                in_tail_position: false,
            }
        };

        // If we have a default, wrap in: (let [result get-expr] (if (nil? result) default result))
        if let Some(default_val) = default {
            let result_local = self.next_local;
            self.next_local += 1;
            Ok(Expr::Let {
                bindings: vec![(result_local, get_expr)],
                body: Box::new(Expr::If {
                    cond: Box::new(Expr::NilCheck(Box::new(Expr::LocalGet {
                        local: result_local,
                        ty: Type::GcRef,
                    }))),
                    then_branch: Box::new(default_val),
                    else_branch: Box::new(Expr::LocalGet {
                        local: result_local,
                        ty: Type::GcRef,
                    }),
                    ty: Type::GcRef,
                }),
            })
        } else {
            Ok(get_expr)
        }
    }

    /// Lower (assoc map key val) -> ProtocolDispatch for IAssociative/-assoc
    fn lower_assoc(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 3 {
            return Err(CompileError::Parse(
                "assoc requires exactly 3 arguments: map, key, and value".into(),
            ));
        }
        // Arguments are never in tail position - only the result of assoc is
        let (map, key, val) = self.with_tail_disabled(|l| {
            Ok((
                l.lower_expr(&args[0])?,
                l.lower_expr(&args[1])?,
                l.lower_expr(&args[2])?,
            ))
        })?;
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

    /// Lower (disj set val) -> call core.sus disj function
    fn lower_disj(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 2 {
            return Err(CompileError::Parse(
                "disj requires exactly 2 arguments: set and value".into(),
            ));
        }
        // Call the core.sus disj function
        self.lower_func_call("disj", args)
    }

    /// Lower (dissoc map key) -> call core.sus dissoc function
    fn lower_dissoc(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 2 {
            return Err(CompileError::Parse(
                "dissoc requires exactly 2 arguments: map and key".into(),
            ));
        }
        // Call the core.sus dissoc function
        self.lower_func_call("dissoc", args)
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
            "rem" | "mod" | "quot" => Some(2),
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
                is_variadic: false,
            });
        }

        // Generate wrapper function name
        let wrapper_name = format!("$builtin_{}", name.replace(|c: char| !c.is_alphanumeric(), "_"));

        // Calculate wrapper function index using closure_counter for consistency
        let wrapper_idx = self.num_deftype_constructors + self.num_deftype_impl_funcs + self.num_analyzed_funcs + self.num_extension_funcs + self.closure_counter;
        self.closure_counter += 1;

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
            rest_param: None,
            body,
            is_variadic: false,
        });

        Ok(Expr::ClosureNew {
            func_idx: wrapper_idx,
            arity,
            captures: vec![], // no captures needed for built-ins
            is_variadic: false,
        })
    }

    /// Create a closure wrapper for a user-defined function used in #'var.
    /// This wraps the user function so it can be stored in a Var and called later.
    fn lower_user_func_as_closure(
        &mut self,
        name: &str,
        _target_func_idx: u32,
        arity: usize,
    ) -> CompileResult<Expr> {
        // Check if we already have a wrapper for this function
        let cache_key = format!("userfn_{}", name);
        if let Some(&func_idx) = self.builtin_wrappers.get(&cache_key) {
            return Ok(Expr::ClosureNew {
                func_idx,
                arity: arity as u32,
                captures: vec![],
                is_variadic: false,
            });
        }

        // Generate wrapper function name
        let wrapper_name = format!("$userfn_{}", name.replace(|c: char| !c.is_alphanumeric(), "_"));

        // Calculate wrapper function index using closure_counter for consistency
        let wrapper_idx = self.num_deftype_constructors
            + self.num_deftype_impl_funcs
            + self.num_analyzed_funcs
            + self.num_extension_funcs
            + self.closure_counter;
        self.closure_counter += 1;

        // Register wrapper function
        self.func_indices.insert(wrapper_name.clone(), wrapper_idx);
        self.builtin_wrappers.insert(cache_key, wrapper_idx);

        // Generate parameter names
        let param_names: Vec<String> = (0..arity).map(|i| format!("$arg{}", i)).collect();

        // Generate wrapper body: a call to the user function
        // (fn [a b] (user-fn a b))
        let body = self.make_user_func_call_body(name, &param_names);

        // Store closure wrapper for later generation
        self.pending_closures.push(ClosureWrapper {
            name: wrapper_name,
            captures: vec![],
            params: param_names,
            rest_param: None,
            body,
            is_variadic: false,
        });

        Ok(Expr::ClosureNew {
            func_idx: wrapper_idx,
            arity: arity as u32,
            captures: vec![],
            is_variadic: false,
        })
    }

    /// Create a variadic closure wrapper for a user-defined variadic function.
    ///
    /// Unlike builtins which use VARIADIC_CLOSURE (limited to 8 args), user-defined
    /// variadic functions use CLOSURE_1 with VARIADIC_CAPTURE type_id to support
    /// arbitrary argument counts via apply.
    fn lower_variadic_user_func_as_closure(
        &mut self,
        name: &str,
        _target_func_idx: u32,
    ) -> CompileResult<Expr> {
        // Check if we already have a wrapper for this variadic function
        let cache_key = format!("variadic_userfn_capture_{}", name);
        if let Some(&wrapper_idx) = self.builtin_wrappers.get(&cache_key) {
            return Ok(Expr::ClosureNew {
                func_idx: wrapper_idx,
                arity: 1, // Takes args_array
                captures: vec![],
                is_variadic: true, // Marks as VARIADIC_CAPTURE
            });
        }

        // Generate a single wrapper function that takes an args array
        // and passes it to the original variadic function
        let wrapper_name = format!(
            "$variadic_userfn_capture_{}",
            name.replace(|c: char| !c.is_alphanumeric(), "_")
        );

        let wrapper_idx = self.num_deftype_constructors
            + self.num_deftype_impl_funcs
            + self.num_analyzed_funcs
            + self.num_extension_funcs
            + self.closure_counter;

        self.closure_counter += 1;
        self.func_indices.insert(wrapper_name.clone(), wrapper_idx);

        // The wrapper takes an args array and needs to call the target variadic
        // function directly with that array. Use $direct-variadic-call to bypass
        // the normal variadic packing that would wrap $args in another array.
        let body = Edn::List(vec![
            Edn::Symbol(Symbol::new("$direct-variadic-call")),
            Edn::Symbol(Symbol::new(name)),
            Edn::Symbol(Symbol::new("$args")),
        ]);

        self.pending_closures.push(ClosureWrapper {
            name: wrapper_name,
            captures: vec![],
            params: vec!["$args".to_string()], // Single array parameter
            rest_param: None,
            body,
            is_variadic: false, // The wrapper itself is fixed-arity (1 param)
        });

        self.builtin_wrappers.insert(cache_key, wrapper_idx);

        Ok(Expr::ClosureNew {
            func_idx: wrapper_idx,
            arity: 1,
            captures: vec![],
            is_variadic: true, // Marks type_id as VARIADIC_CAPTURE for apply
        })
    }

    /// Create a call body for wrapping a user function.
    fn make_user_func_call_body(&self, func_name: &str, params: &[String]) -> Edn {
        // Create a call expression: (func-name $arg0 $arg1 ...)
        let mut items = vec![Edn::Symbol(suss_core::Symbol::new(func_name))];
        for param in params {
            items.push(Edn::Symbol(suss_core::Symbol::new(param)));
        }
        Edn::List(items)
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
        // Use closure_counter for base index for consistency
        let base_idx = self.num_deftype_constructors + self.num_deftype_impl_funcs + self.num_analyzed_funcs + self.num_extension_funcs + self.closure_counter;

        for arity in 0..=8u32 {
            let wrapper_name = format!(
                "$variadic_{}_{}",
                name.replace(|c: char| !c.is_alphanumeric(), "_"),
                arity
            );

            // Calculate function index
            let func_idx = base_idx + arity;
            self.closure_counter += 1;
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
                rest_param: None,
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
        // Check built-in methods first (some like -invoke depend on arity)
        if let Some(id) = self.builtin_method_id_with_arity(method_name, arity) {
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
        // Track maximum method ID for dispatch table sizing
        self.module.max_method_id = self.module.max_method_id.max(id);
        id
    }

    /// Map built-in method names to their method IDs.
    /// Most methods have fixed IDs, but -invoke depends on arity.
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
            // -invoke is arity-dependent, handled separately
            _ => None,
        }
    }

    /// Map built-in method names to their method IDs, with arity for multi-arity methods.
    fn builtin_method_id_with_arity(&self, method_name: &str, arity: usize) -> Option<u32> {
        // First check non-arity-dependent methods
        if let Some(id) = self.builtin_method_id(method_name) {
            return Some(id);
        }
        // Handle arity-dependent methods
        match (method_name, arity) {
            // -invoke: (coll key) = 2 args -> INVOKE_1, (coll key not-found) = 3 args -> INVOKE_2
            ("-invoke", 2) => Some(method_ids::INVOKE_1),
            ("-invoke", 3) => Some(method_ids::INVOKE_2),
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
        let method_id = if let Some(id) = self.builtin_method_id_with_arity(name, arity) {
            // Built-in methods have fixed IDs (some depend on arity like -invoke)
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
                    is_mutable: f.is_mutable,
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
                    is_mutable: f.is_mutable,
                }).collect(),
                gc_type_idx,
                type_id,
            };
            self.module.deftypes.push(deftype_def);

            // Generate constructor function ->TypeName
            // Skip constructors for HAMT node types (gc_type_idx 5-7) since they use
            // specific array types (ref $ARRAY) that require special handling
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
                    // Unbox: (struct.get $INT64 1 (ref.cast ... arg))
                    Expr::StructGet {
                        type_idx: gc_types::INT64,
                        field_idx: gc_types::I64_VALUE,
                        value: Box::new(local_get),
                    }
                }
                FieldType::F64 => {
                    // Unbox: (struct.get $FLOAT 1 (ref.cast ... arg))
                    Expr::StructGet {
                        type_idx: gc_types::FLOAT64,
                        field_idx: gc_types::F64_VALUE,
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
            rest_param: None,
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
            "LargeInt" => return Ok(type_ids::INT64 as u32),
            "Float" => return Ok(type_ids::FLOAT64 as u32),
            _ => {}
        }

        // Check user-defined types (includes collection types from core.sus:
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
            rest_param: None,
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
    /// Gets an element from a ARRAY array at the given index.
    fn lower_aget(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 2 {
            return Err(CompileError::Parse(
                "aget requires exactly 2 arguments: array and index".into(),
            ));
        }
        let (array, index) = self.with_tail_disabled(|l| {
            Ok((l.lower_expr(&args[0])?, l.lower_expr(&args[1])?))
        })?;

        // Use ARRAY as the default array type
        Ok(Expr::ArrayGet {
            type_idx: gc_types::ARRAY,
            array: Box::new(array),
            index: Box::new(index),
        })
    }

    /// Lower (aset arr idx val) -> ArraySet
    /// Sets an element in a ARRAY array at the given index.
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

        // Use ARRAY as the default array type
        Ok(Expr::ArraySet {
            type_idx: gc_types::ARRAY,
            array: Box::new(array),
            index: Box::new(index),
            value: Box::new(value),
        })
    }

    /// Lower (alength arr) -> ArrayLen
    /// Gets the length of a ARRAY array.
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
    /// Creates a shallow copy of a ARRAY array.
    fn lower_aclone(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 1 {
            return Err(CompileError::Parse(
                "aclone requires exactly 1 argument: array".into(),
            ));
        }
        let array = self.with_tail_disabled(|l| l.lower_expr(&args[0]))?;

        // Use ARRAY as the default array type
        Ok(Expr::ArrayClone {
            type_idx: gc_types::ARRAY,
            array: Box::new(array),
        })
    }

    /// Lower (acopy dst dst-offset src src-offset len) -> ArrayCopy
    /// Copies elements between ARRAY arrays. Returns nil.
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

        // Use ARRAY as the default array type
        Ok(Expr::ArrayCopy {
            type_idx: gc_types::ARRAY,
            dst: Box::new(dst),
            dst_offset: Box::new(dst_offset),
            src: Box::new(src),
            src_offset: Box::new(src_offset),
            len: Box::new(len),
        })
    }

    /// Lower (make-array size) -> ArrayNewDefault
    /// Creates a new ARRAY array with null values.
    fn lower_make_array(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 1 {
            return Err(CompileError::Parse(
                "make-array requires exactly 1 argument: size".into(),
            ));
        }
        let size = self.with_tail_disabled(|l| l.lower_expr(&args[0]))?;

        // Use ARRAY as the default array type
        Ok(Expr::ArrayNewDefault {
            type_idx: gc_types::ARRAY,
            size: Box::new(size),
        })
    }

    /// Lower (make-string size) -> ArrayNewDefault for STRING type
    /// Creates a new STRING array (array<i8>) with zero bytes.
    fn lower_make_string(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 1 {
            return Err(CompileError::Parse(
                "make-string requires exactly 1 argument: size".into(),
            ));
        }
        let size = self.with_tail_disabled(|l| l.lower_expr(&args[0]))?;

        // Use STRING as the array type (array<i8>)
        Ok(Expr::ArrayNewDefault {
            type_idx: gc_types::STRING,
            size: Box::new(size),
        })
    }

    /// Lower (scopy dst dst-offset src src-offset len) -> ArrayCopy for STRING type
    /// Copies bytes between STRING arrays.
    fn lower_scopy(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 5 {
            return Err(CompileError::Parse(
                "scopy requires exactly 5 arguments: dst dst-offset src src-offset len".into(),
            ));
        }
        let dst = self.with_tail_disabled(|l| l.lower_expr(&args[0]))?;
        let dst_offset = self.with_tail_disabled(|l| l.lower_expr(&args[1]))?;
        let src = self.with_tail_disabled(|l| l.lower_expr(&args[2]))?;
        let src_offset = self.with_tail_disabled(|l| l.lower_expr(&args[3]))?;
        let len = self.with_tail_disabled(|l| l.lower_expr(&args[4]))?;

        // Use STRING as the array type (array<i8>)
        Ok(Expr::ArrayCopy {
            type_idx: gc_types::STRING,
            dst: Box::new(dst),
            dst_offset: Box::new(dst_offset),
            src: Box::new(src),
            src_offset: Box::new(src_offset),
            len: Box::new(len),
        })
    }

    /// Lower (sget string index) -> ArrayGet for STRING type
    /// Gets a byte from a STRING array.
    fn lower_sget(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 2 {
            return Err(CompileError::Parse(
                "sget requires exactly 2 arguments: string and index".into(),
            ));
        }
        let (array, index) = self.with_tail_disabled(|l| {
            Ok((l.lower_expr(&args[0])?, l.lower_expr(&args[1])?))
        })?;

        // Use STRING as the array type (array<i8>)
        Ok(Expr::ArrayGet {
            type_idx: gc_types::STRING,
            array: Box::new(array),
            index: Box::new(index),
        })
    }

    /// Lower (sset string index value) -> ArraySet for STRING type
    /// Sets a byte in a STRING array.
    fn lower_sset(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 3 {
            return Err(CompileError::Parse(
                "sset requires exactly 3 arguments: string, index, and value".into(),
            ));
        }
        let (array, index, value) = self.with_tail_disabled(|l| {
            Ok((
                l.lower_expr(&args[0])?,
                l.lower_expr(&args[1])?,
                l.lower_expr(&args[2])?,
            ))
        })?;

        // Use STRING as the array type (array<i8>)
        Ok(Expr::ArraySet {
            type_idx: gc_types::STRING,
            array: Box::new(array),
            index: Box::new(index),
            value: Box::new(value),
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

    /// Lower (int? x) -> IntCheck
    /// Tests if value is an integer (i31ref with number tag OR INT64 struct).
    fn lower_int_check(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 1 {
            return Err(CompileError::Parse(
                "int? requires exactly 1 argument".into(),
            ));
        }
        let value = self.with_tail_disabled(|l| l.lower_expr(&args[0]))?;

        Ok(Expr::IntCheck(Box::new(value)))
    }

    /// Lower (float? x) -> FloatCheck
    /// Tests if value is a float (FLOAT64 struct).
    fn lower_float_check(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 1 {
            return Err(CompileError::Parse(
                "float? requires exactly 1 argument".into(),
            ));
        }
        let value = self.with_tail_disabled(|l| l.lower_expr(&args[0]))?;

        Ok(Expr::FloatCheck(Box::new(value)))
    }

    /// Lower (string? x) -> StringCheck
    /// Tests if value is a string (STRING array).
    fn lower_string_check(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 1 {
            return Err(CompileError::Parse(
                "string? requires exactly 1 argument".into(),
            ));
        }
        let value = self.with_tail_disabled(|l| l.lower_expr(&args[0]))?;

        Ok(Expr::StringCheck(Box::new(value)))
    }

    /// Lower (trunc x) -> F64Trunc
    /// Truncates f64 toward zero (removes fractional part).
    fn lower_f64_trunc(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 1 {
            return Err(CompileError::Parse(
                "trunc requires exactly 1 argument".into(),
            ));
        }
        let value = self.with_tail_disabled(|l| l.lower_expr(&args[0]))?;

        Ok(Expr::F64Trunc(Box::new(value)))
    }

    /// Lower (f64->i64 x) -> F64ToI64
    /// Converts f64 to i64 (truncated toward zero).
    fn lower_f64_to_i64(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 1 {
            return Err(CompileError::Parse(
                "f64->i64 requires exactly 1 argument".into(),
            ));
        }
        let value = self.with_tail_disabled(|l| l.lower_expr(&args[0]))?;

        Ok(Expr::F64ToI64(Box::new(value)))
    }

    /// Lower (i64->f64 x) -> I64ToF64
    /// Converts i64 to f64.
    fn lower_i64_to_f64(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 1 {
            return Err(CompileError::Parse(
                "i64->f64 requires exactly 1 argument".into(),
            ));
        }
        let value = self.with_tail_disabled(|l| l.lower_expr(&args[0]))?;

        Ok(Expr::I64ToF64(Box::new(value)))
    }

    /// Lower (sqrt x) -> F64Sqrt
    /// Computes the square root of a float.
    fn lower_f64_sqrt(&mut self, args: &[Edn]) -> CompileResult<Expr> {
        if args.len() != 1 {
            return Err(CompileError::Parse(
                "sqrt requires exactly 1 argument".into(),
            ));
        }
        let value = self.with_tail_disabled(|l| l.lower_expr(&args[0]))?;

        Ok(Expr::F64Sqrt(Box::new(value)))
    }
}
