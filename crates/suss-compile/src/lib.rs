//! Suss Static Compiler
//!
//! Compiles Suss source code to WASM components implementing user-specified WIT worlds.
//!
//! # Compilable Subset
//!
//! The compiler supports a subset of Suss suitable for static compilation:
//!
//! - `def` - Top-level constants
//! - `defn` - Named functions (with `^:export` metadata for WIT exports)
//! - `fn` - Lambda expressions (no mutable capture)
//! - `let`, `if`, `do` - Control flow
//! - `loop/recur` - Loops (maps to WASM loops)
//! - Numbers - i32, i64, f64 (no BigInt)
//! - Strings - Linear memory strings
//! - Vectors - Linear memory arrays
//!
//! # Example
//!
//! ```ignore
//! use suss_compile::Compiler;
//!
//! let source = r#"
//!     (defn ^:export greet [name]
//!       (str "Hello, " name "!"))
//! "#;
//!
//! let wit = r#"
//!     package example:hello;
//!     world hello {
//!         export greet: func(name: string) -> string;
//!     }
//! "#;
//!
//! let compiler = Compiler::new();
//! let wasm = compiler.compile(source, wit)?;
//! ```

mod analyze;
mod codegen;
mod component;
mod config;
mod error;
mod eval;
mod expand;
mod ir;
mod lower;
mod wasi;
pub mod worlds;
pub mod runtime_abi;
pub mod portable;

// Native compiled phase/session host for compiler embedding and CLI paths.
// Wasm target assembly remains in portable; these modules require Wasmtime.
#[cfg(not(target_family = "wasm"))]
mod portable_module_cache;
#[cfg(not(target_family = "wasm"))]
mod portable_defn;
#[cfg(not(target_family = "wasm"))]
pub mod portable_macro_data;
#[cfg(not(target_family = "wasm"))]
pub mod portable_macro_graph;
#[cfg(not(target_family = "wasm"))]
pub mod portable_macros;
#[cfg(not(target_family = "wasm"))]
pub mod portable_repl;
#[cfg(not(target_family = "wasm"))]
pub mod portable_session;
#[cfg(not(target_family = "wasm"))]
pub mod portable_aot;
#[cfg(not(target_family = "wasm"))]
pub mod portable_expression;
#[cfg(not(target_family = "wasm"))]
pub mod portable_project;

pub use config::{SussConfig, WorldConfig};
pub use error::{CompileError, CompileResult};

/// Result of compiling an expression
#[derive(Debug)]
pub struct CompiledExpr {
    /// Entry module bytes. Shared-ABI expressions also require the dependency
    /// bundle in `prepared`; use `execute` rather than instantiating these alone.
    pub wasm: Vec<u8>,
    /// Whether this is a WASM Component (true) or core module (false)
    pub is_component: bool,
    #[cfg(not(target_family = "wasm"))]
    pub prepared: Option<portable_expression::ExpressionArtifact>,
}

#[cfg(not(target_family = "wasm"))]
impl CompiledExpr {
    pub fn execute(
        self,
        session: &mut portable_session::Session,
    ) -> Result<Option<portable_session::SessionValue>, portable_session::SessionError> {
        self.execute_with_core(session, |_| Ok(())).map(|(value, ())| value)
    }

    pub fn execute_with_core<T>(
        self,
        session: &mut portable_session::Session,
        core_ready: impl FnOnce(&mut portable_session::Session) -> Result<T, portable_session::SessionError>,
    ) -> Result<(Option<portable_session::SessionValue>, T), portable_session::SessionError> {
        self.prepared.ok_or_else(|| portable_session::SessionError::Host(
            wasmtime::Error::msg("This prototype artifact has no shared-ABI expression bundle"),
        ))?.execute_with_core(session, core_ready)
    }
}

/// Bundled WASI version
pub const BUNDLED_WASI_VERSION: &str = wasi::WASI_VERSION;

use suss_core::Edn;
use suss_reader::ParserState;
use wit_parser::Resolve;

/// Bundled core.sus source - automatically loaded before user code (per Clojure semantics)
const CORE_SOURCE: &str = include_str!("core.sus");

/// Cached core.sus analysis for REPL performance
///
/// Core.sus is ~2000 lines and takes significant time to parse, expand, and analyze.
/// By caching this work, we can avoid repeating it on every REPL expression.
#[derive(Clone)]
pub struct CoreCache {
    /// Parsed core.sus expressions (before macro expansion)
    pub parsed: Vec<Edn>,
    /// Analyzed function definitions from core.sus
    pub functions: Vec<analyze::AnalyzedFunction>,
    /// Analyzed deftype definitions from core.sus
    pub deftypes: Vec<analyze::AnalyzedDeftype>,
    /// Analyzed protocol definitions from core.sus
    pub protocols: Vec<analyze::AnalyzedProtocol>,
    /// Analyzed extend-type definitions from core.sus
    pub extensions: Vec<analyze::AnalyzedExtension>,
}

/// The Suss static compiler
///
/// For REPL usage, create a single Compiler instance and reuse it across expressions.
/// This enables core.sus caching for significant performance improvement.
pub struct Compiler {
    /// Cached core.sus analysis (lazily initialized)
    core_cache: Option<CoreCache>,
}

/// Parse core.sus and return its expressions
fn load_core_exprs() -> CompileResult<Vec<Edn>> {
    // Handle empty/comments-only core.sus gracefully
    let trimmed = CORE_SOURCE
        .lines()
        .filter(|line| {
            let line = line.trim();
            !line.is_empty() && !line.starts_with(";;")
        })
        .collect::<Vec<_>>()
        .join("\n");

    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let mut parser_state = ParserState::new("core");
    let result = suss_reader::parse_all(CORE_SOURCE, &mut parser_state)
        .map_err(|e| CompileError::Parse(format!("core.sus: {}", e)))?;

    Ok(result)
}

/// Inject the `suss` runtime interface into a WIT source string.
///
/// The compiler unconditionally imports `suss:print_str` for string output.
/// This function adds the interface definition before the `world` keyword
/// and adds `import suss;` inside the world body, so the component encoder
/// can match the module import against the WIT world.
fn inject_suss_interface(wit_source: &str) -> String {
    // Interface definition to inject before the world
    let iface_def = "\ninterface suss {\n    print-str: func(ptr: u32, len: u32);\n}\n";
    let import_stmt = "    import suss;\n";

    let mut result = String::with_capacity(wit_source.len() + iface_def.len() + import_stmt.len());

    // Find the `world` keyword and inject the interface before it
    if let Some(world_pos) = wit_source.find("\nworld ") {
        result.push_str(&wit_source[..world_pos]);
        result.push_str(iface_def);
        let rest = &wit_source[world_pos..];

        // Find the opening `{` of the world and inject `import suss;` after it
        if let Some(brace_pos) = rest.find('{') {
            result.push_str(&rest[..brace_pos + 1]);
            result.push('\n');
            result.push_str(import_stmt);
            result.push_str(&rest[brace_pos + 1..]);
        } else {
            result.push_str(rest);
        }
    } else {
        // Fallback: try without leading newline (world at start of file)
        if let Some(world_pos) = wit_source.find("world ") {
            result.push_str(&wit_source[..world_pos]);
            result.push_str(iface_def);
            let rest = &wit_source[world_pos..];
            if let Some(brace_pos) = rest.find('{') {
                result.push_str(&rest[..brace_pos + 1]);
                result.push('\n');
                result.push_str(import_stmt);
                result.push_str(&rest[brace_pos + 1..]);
            } else {
                result.push_str(rest);
            }
        } else {
            // No world keyword found, return as-is
            return wit_source.to_string();
        }
    }

    result
}

impl Compiler {
    /// Prepare host-owned ABI2 expression artifacts through isolated compiled
    /// macros. Runtime initializers run only when the returned artifact executes.
    /// The legacy byte-only expression methods remain pending migration.
    #[cfg(not(target_family = "wasm"))]
    pub fn prepare_expression(
        &mut self,
        source: &str,
        source_paths: &[std::path::PathBuf],
    ) -> CompileResult<portable_expression::ExpressionArtifact> {
        portable_expression::prepare(source, source_paths)
            .map_err(|error| CompileError::Semantic(error.to_string()))
    }
    /// Create a new compiler instance
    ///
    /// For REPL usage, create one Compiler and reuse it for all expressions.
    /// This enables core.sus caching.
    pub fn new() -> Self {
        Self { core_cache: None }
    }

    /// Ensure core.sus is loaded and cached
    ///
    /// This parses, expands, and analyzes core.sus once, caching the results.
    /// Subsequent calls return the cached data immediately.
    pub fn ensure_core_loaded(&mut self) -> CompileResult<&CoreCache> {
        if self.core_cache.is_none() {
            // Parse core.sus
            let core_exprs = load_core_exprs()?;

            // Expand macros
            let expanded = expand::expand_all(core_exprs.clone(), None)?;

            // Extract definitions
            let (functions, deftypes, protocols, extensions, _remaining) =
                Self::extract_core_definitions(expanded)?;

            self.core_cache = Some(CoreCache {
                parsed: core_exprs,
                functions,
                deftypes,
                protocols,
                extensions,
            });
        }
        Ok(self.core_cache.as_ref().unwrap())
    }

    /// Get the cached core.sus data without loading
    ///
    /// Returns None if core.sus hasn't been loaded yet.
    pub fn get_core_cache(&self) -> Option<&CoreCache> {
        self.core_cache.as_ref()
    }

    /// Preload core.sus cache
    ///
    /// Call this during REPL startup to front-load the parsing cost.
    pub fn preload_core(&mut self) -> CompileResult<()> {
        self.ensure_core_loaded()?;
        Ok(())
    }

    /// Compile an expression bundle using the cached compiled core bootstrap.
    /// User macros execute in an isolated compiled phase; Runtime initialization
    /// is deferred. This is a fresh compilation, not a source-replaying REPL.
    #[cfg(not(target_family = "wasm"))]
    pub fn compile_expr_cached(&mut self, expr_source: &str) -> CompileResult<CompiledExpr> {
        let prepared = self.prepare_expression(expr_source, &[std::path::PathBuf::from("src")])?;
        let wasm = prepared.modules().last().expect("expression bootstrap module").to_vec();
        Ok(CompiledExpr { wasm, is_component: false, prepared: Some(prepared) })
    }

    /// Prototype compiler-on-Wasm route, pending compiled host support.
    ///
    /// This method uses pre-analyzed core.sus definitions, avoiding the cost of
    /// re-parsing and re-analyzing core.sus on every expression. For a typical
    /// REPL session, this provides ~50-70% speedup.
    ///
    /// # Arguments
    ///
    /// * `expr_source` - The user's expression(s) to compile
    ///
    /// # Returns
    ///
    /// Compiled WASM bytes and metadata
    #[cfg(target_family = "wasm")]
    pub fn compile_expr_cached(&mut self, expr_source: &str) -> CompileResult<CompiledExpr> {
        // Ensure core.sus is cached
        let core = self.ensure_core_loaded()?.clone();

        // Parse only user expressions
        let mut parser_state = ParserState::new("suss");
        let user_exprs = suss_reader::parse_all(expr_source, &mut parser_state)
            .map_err(|e| CompileError::Parse(e.to_string()))?;

        // Combine core + user for macro expansion (macros may reference each other)
        let mut all_exprs = core.parsed.clone();
        all_exprs.extend(user_exprs);

        // Expand macros on combined expressions
        let expanded = expand::expand_all(all_exprs, None)?;

        // Extract user definitions (skip core definitions which are already in cache)
        // We need to re-extract to get the expanded user definitions
        let (all_fns, all_deftypes, all_protocols, all_extensions, user_expr) =
            Self::extract_core_definitions(expanded)?;

        // Partition: core definitions (by name) vs user definitions
        // User definitions may override core definitions, so we take all and dedupe later
        let core_fn_names: std::collections::HashSet<_> =
            core.functions.iter().map(|f| f.name.as_str()).collect();

        let mut functions = core.functions.clone();
        for func in all_fns {
            if !core_fn_names.contains(func.name.as_str()) {
                functions.push(func);
            }
        }

        let core_deftype_names: std::collections::HashSet<_> =
            core.deftypes.iter().map(|d| d.name.as_str()).collect();

        let mut deftypes = core.deftypes.clone();
        for dt in all_deftypes {
            if !core_deftype_names.contains(dt.name.as_str()) {
                deftypes.push(dt);
            }
        }

        let core_protocol_names: std::collections::HashSet<_> =
            core.protocols.iter().map(|p| p.name.as_str()).collect();

        let mut protocols = core.protocols.clone();
        for proto in all_protocols {
            if !core_protocol_names.contains(proto.name.as_str()) {
                protocols.push(proto);
            }
        }

        // Extensions are additive (can extend same type multiple times)
        let mut extensions = core.extensions.clone();
        extensions.extend(all_extensions);

        // Detect WASI calls in the expression
        let wasi_calls = wasi::collect_wasi_calls(&user_expr);

        if wasi_calls.is_empty() {
            // No WASI calls - compile as core module
            let wasm = self.compile_expr_core_from_analyzed(
                user_expr, functions, deftypes, protocols, extensions
            )?;
            Ok(CompiledExpr {
                wasm,
                is_component: false,
                #[cfg(not(target_family = "wasm"))]
                prepared: None,
            })
        } else {
            // WASI calls detected - compile as component with imports
            let wasm = self.compile_expr_with_wasi_from_analyzed(
                user_expr, wasi_calls, functions, deftypes, protocols, extensions
            )?;
            Ok(CompiledExpr {
                wasm,
                is_component: true,
                #[cfg(not(target_family = "wasm"))]
                prepared: None,
            })
        }
    }

    /// Compile expression as core WASM module from pre-analyzed definitions
    #[cfg(target_family = "wasm")]
    fn compile_expr_core_from_analyzed(
        &mut self,
        expr: Edn,
        mut functions: Vec<analyze::AnalyzedFunction>,
        deftypes: Vec<analyze::AnalyzedDeftype>,
        protocols: Vec<analyze::AnalyzedProtocol>,
        extensions: Vec<analyze::AnalyzedExtension>,
    ) -> CompileResult<Vec<u8>> {
        // Infer the return type
        let return_type = analyze::infer_expr_type(&expr)?;

        // Add eval function
        functions.push(analyze::AnalyzedFunction {
            name: "__eval".to_string(),
            exported: true,
            export_name: Some("eval".to_string()),
            params: Vec::new(),
            rest_param: None,
            return_type,
            return_type_hint: None,
            docstring: None,
            body: expr,
        });

        let analyzed = analyze::AnalyzedModule {
            namespace: None,
            world_target: None,
            imports: Vec::new(),
            suss_requires: Vec::new(),
            functions,
            globals: Vec::new(),
            protocols,
            extensions,
            deftypes,
        };

        // Lower to IR
        let ir = lower::lower(&analyzed)?;

        // Generate WASM
        codegen::generate_module(&ir)
    }

    /// Compile expression as WASM Component with WASI imports from pre-analyzed definitions
    #[cfg(target_family = "wasm")]
    fn compile_expr_with_wasi_from_analyzed(
        &mut self,
        expr: Edn,
        wasi_calls: Vec<wasi::WasiFunctionInfo>,
        mut functions: Vec<analyze::AnalyzedFunction>,
        deftypes: Vec<analyze::AnalyzedDeftype>,
        protocols: Vec<analyze::AnalyzedProtocol>,
        extensions: Vec<analyze::AnalyzedExtension>,
    ) -> CompileResult<Vec<u8>> {
        // Infer the return type
        let return_type = self.infer_expr_type_with_wasi(&expr, &wasi_calls)?;

        // Convert WASI calls to AnalyzedImports
        let imports: Vec<analyze::AnalyzedImport> = wasi_calls
            .iter()
            .map(|info| {
                let package = info
                    .wit_interface
                    .split(':')
                    .nth(1)
                    .and_then(|s| s.split('/').next())
                    .unwrap_or("unknown");

                analyze::AnalyzedImport {
                    alias: format!("wasi.{}", package),
                    wit_interface: info.wit_interface.clone(),
                    function_name: info.function_name.clone(),
                    params: info.params.clone(),
                    return_type: info.return_type.clone(),
                }
            })
            .collect();

        // Add eval function
        functions.push(analyze::AnalyzedFunction {
            name: "__eval".to_string(),
            exported: true,
            export_name: Some("eval".to_string()),
            params: Vec::new(),
            rest_param: None,
            return_type,
            return_type_hint: None,
            docstring: None,
            body: expr,
        });

        let analyzed = analyze::AnalyzedModule {
            namespace: None,
            world_target: None,
            imports,
            suss_requires: Vec::new(),
            functions,
            globals: Vec::new(),
            protocols,
            extensions,
            deftypes,
        };

        // Lower to IR
        let ir = lower::lower(&analyzed)?;

        // Generate WASM Component with imports
        codegen::generate_component_with_imports(&ir)
    }

    /// Compile a single expression to a WASM module or component
    ///
    /// This creates a minimal WASM module with a single exported function `eval`
    /// that returns the result of the expression. No WIT file is required.
    ///
    /// If the expression contains WASI calls (e.g., `(wasi.random/get-random-u64)`),
    /// returns a WASM Component with appropriate imports. Otherwise returns a core module.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let compiler = Compiler::new();
    /// let wasm = compiler.compile_expr("(+ 1 2)")?;
    /// // Run with wasmtime, call `eval` function, get result
    /// ```
    pub fn compile_expr(&mut self, expr_source: &str) -> CompileResult<Vec<u8>> {
        let result = self.compile_expr_with_info(expr_source)?;
        Ok(result.wasm)
    }

    /// Compile a single expression and return info about the result
    ///
    /// Returns both the WASM bytes and whether it's a component (WASI) or core module.
    pub fn compile_expr_with_info(&mut self, expr_source: &str) -> CompileResult<CompiledExpr> {
        // Load core.sus (auto-injected before user code per Clojure semantics)
        let core_exprs = load_core_exprs()?;

        // Parse all expressions in the source
        let mut parser_state = ParserState::new("suss");
        let user_exprs = suss_reader::parse_all(expr_source, &mut parser_state)
            .map_err(|e| CompileError::Parse(e.to_string()))?;

        // Combine core + user expressions
        let mut all_exprs = core_exprs;
        all_exprs.extend(user_exprs);

        // Expand macros on combined expressions
        let expanded = expand::expand_all(all_exprs, None)?;

        // Separate defn/deftype/extend-type forms from the final expression
        let (core_fns, deftypes, protocols, extensions, user_expr) = Self::extract_core_definitions(expanded)?;

        // Detect WASI calls in the expression
        let wasi_calls = wasi::collect_wasi_calls(&user_expr);

        if wasi_calls.is_empty() {
            // No WASI calls - compile as core module (existing behavior)
            let wasm = self.compile_expr_core(user_expr, core_fns, deftypes, protocols, extensions)?;
            Ok(CompiledExpr {
                wasm,
                is_component: false,
                #[cfg(not(target_family = "wasm"))]
                prepared: None,
            })
        } else {
            // WASI calls detected - compile as component with imports
            let wasm = self.compile_expr_with_wasi(user_expr, wasi_calls, core_fns, deftypes, protocols, extensions)?;
            Ok(CompiledExpr {
                wasm,
                is_component: true,
                #[cfg(not(target_family = "wasm"))]
                prepared: None,
            })
        }
    }

    /// Extract defn, deftype, defprotocol, and extend-type forms from expressions
    /// Returns (functions, deftypes, protocols, extensions, final_expr)
    fn extract_core_definitions(
        exprs: Vec<Edn>,
    ) -> CompileResult<(
        Vec<analyze::AnalyzedFunction>,
        Vec<analyze::AnalyzedDeftype>,
        Vec<analyze::AnalyzedProtocol>,
        Vec<analyze::AnalyzedExtension>,
        Edn,
    )> {
        let mut functions = Vec::new();
        let mut deftypes = Vec::new();
        let mut protocols = Vec::new();
        let mut extensions = Vec::new();
        let mut remaining = Vec::new();

        for expr in exprs {
            if let Edn::List(ref items) = expr {
                if let Some(Edn::Symbol(sym)) = items.first() {
                    // Skip namespace declarations (metadata only)
                    if sym.name == "ns" {
                        continue;
                    }
                    // Handle (do ...) blocks by recursively extracting definitions
                    if sym.name == "do" && items.len() > 1 {
                        let inner_exprs: Vec<Edn> = items[1..].to_vec();
                        let (inner_fns, inner_deftypes, inner_protocols, inner_extensions, inner_remaining) =
                            Self::extract_core_definitions(inner_exprs)?;
                        functions.extend(inner_fns);
                        deftypes.extend(inner_deftypes);
                        protocols.extend(inner_protocols);
                        extensions.extend(inner_extensions);
                        // Keep the remaining expressions in a do block (or just the expression if single)
                        if inner_remaining != Edn::Nil {
                            remaining.push(inner_remaining);
                        }
                        continue;
                    }
                    // Extract protocol declarations (needed for return type hints)
                    if sym.name == "defprotocol" {
                        if let Some(protocol) = Self::extract_protocol(items)? {
                            protocols.push(protocol);
                            continue;
                        }
                    }
                    // Extract extend-type forms
                    if sym.name == "extend-type" && items.len() >= 2 {
                        if let Some(extension) = Self::extract_extension(items)? {
                            extensions.push(extension);
                            continue;
                        }
                    }
                    // Extract deftype forms
                    if sym.name == "deftype" && items.len() >= 3 {
                        if let Some(deftype) = Self::extract_deftype(items)? {
                            deftypes.push(deftype);
                            continue;
                        }
                    }
                    // Handle (def name value) or (def name "docstring" (fn ...)) forms
                    if sym.name == "def" && items.len() >= 3 {
                        // Skip metadata symbols (^:export, etc.)
                        let mut idx = 1;
                        let mut is_exported = false;
                        while idx < items.len() {
                            if let Edn::Symbol(s) = &items[idx] {
                                if s.name.starts_with('^') {
                                    if s.name == "^:export" {
                                        is_exported = true;
                                    }
                                    idx += 1;
                                    continue;
                                }
                            }
                            break;
                        }
                        if let Some(Edn::Symbol(name_sym)) = items.get(idx) {
                            idx += 1;

                            // Check for optional docstring after name
                            let docstring = if let Some(Edn::String(doc)) = items.get(idx) {
                                idx += 1;
                                Some(doc.clone())
                            } else {
                                None
                            };

                            if let Some(value) = items.get(idx) {
                                // Check if value is an fn form: (def name [docstring] (fn [params] body))
                                if let Edn::List(fn_items) = value {
                                    if let Some(Edn::Symbol(fn_sym)) = fn_items.first() {
                                        if fn_sym.name == "fn" && fn_items.len() >= 3 {
                                            if let Some(mut extracted) = Self::extract_def_fn(
                                                &name_sym.name,
                                                is_exported,
                                                &fn_items[1..],
                                            )? {
                                                // Preserve docstring from defn expansion
                                                extracted.docstring = docstring;
                                                functions.push(extracted);
                                                continue;
                                            }
                                        }
                                    }
                                }
                                // Plain def: (def name value) - wrap as zero-arg fn
                                functions.push(analyze::AnalyzedFunction {
                                    name: name_sym.name.clone(),
                                    exported: is_exported,
                                    export_name: None,
                                    params: vec![],
                                    rest_param: None,
                                    return_type: ir::Type::GcRef,
                                    return_type_hint: None,
                                    docstring,
                                    body: value.clone(),
                                });
                                continue;
                            }
                        }
                    }
                    // Legacy: Handle raw defn forms (for backwards compatibility)
                    if sym.name == "defn" && items.len() >= 3 {
                        // Extract: (defn name [params...] body...)
                        //      or: (defn name "docstring" [params...] body...)
                        if let Edn::Symbol(name_sym) = &items[1] {
                            // Find params vector (skip optional docstring)
                            let (params_idx, body_start, docstring) = if let Edn::String(doc) = &items[2] {
                                // Has docstring: (defn name "doc" [params] body...)
                                (3, 4, Some(doc.clone()))
                            } else {
                                // No docstring: (defn name [params] body...)
                                (2, 3, None)
                            };

                            if params_idx < items.len() {
                                if let Edn::Vector(params_vec) = &items[params_idx] {
                                    // Parse params, handling & for variadic
                                    let mut params: Vec<(String, ir::Type)> = Vec::new();
                                    let mut rest_param: Option<String> = None;
                                    let mut found_amp = false;

                                    for p in params_vec {
                                        if let Edn::Symbol(s) = p {
                                            if s.name == "&" {
                                                found_amp = true;
                                            } else if found_amp {
                                                rest_param = Some(s.name.clone());
                                                break;
                                            } else {
                                                params.push((s.name.clone(), ir::Type::GcRef));
                                            }
                                        }
                                    }

                                    // Body is either single expr or implicit do
                                    let body = if items.len() == body_start + 1 {
                                        items[body_start].clone()
                                    } else if items.len() > body_start {
                                        // Wrap multiple body expressions in do
                                        Edn::List(
                                            std::iter::once(Edn::Symbol(suss_core::Symbol::new(
                                                "do",
                                            )))
                                            .chain(items[body_start..].iter().cloned())
                                            .collect(),
                                        )
                                    } else {
                                        // No body - return nil
                                        Edn::Nil
                                    };

                                    functions.push(analyze::AnalyzedFunction {
                                        name: name_sym.name.clone(),
                                        exported: false,
                                        export_name: None,
                                        params,
                                        rest_param,
                                        return_type: ir::Type::GcRef,
                                        return_type_hint: None,
                                        docstring,
                                        body,
                                    });
                                    continue;
                                }
                            }
                        }
                    }
                }
            }
            remaining.push(expr);
        }

        // The final expression is all remaining expressions combined
        // If no expressions remain (file with only definitions), return nil
        let user_expr = if remaining.is_empty() {
            Edn::Nil
        } else if remaining.len() == 1 {
            remaining.pop().unwrap()
        } else {
            // Wrap multiple expressions in a do block
            Edn::List(
                std::iter::once(Edn::Symbol(suss_core::Symbol::new("do")))
                    .chain(remaining.into_iter())
                    .collect(),
            )
        };

        Ok((functions, deftypes, protocols, extensions, user_expr))
    }

    /// Extract a defprotocol form into an AnalyzedProtocol
    fn extract_protocol(items: &[Edn]) -> CompileResult<Option<analyze::AnalyzedProtocol>> {
        // (defprotocol Name
        //   "optional docstring"
        //   (^type -method [params]))
        if items.len() < 2 {
            return Ok(None);
        }

        // Parse protocol name
        let name = match &items[1] {
            Edn::Symbol(s) => s.name.clone(),
            _ => return Ok(None),
        };

        // Parse method signatures (skip docstrings)
        let mut methods = Vec::new();
        for item in &items[2..] {
            if let Edn::List(method_items) = item {
                if method_items.is_empty() {
                    continue;
                }

                // Parse method with optional return type hint
                // Format: (^type -method [params]) or (-method [params])
                let (method_name, return_type, start_idx) = match &method_items[0] {
                    Edn::Symbol(s) if s.name.starts_with('^') => {
                        // ^type hint before method name
                        let type_hint = s.name[1..].to_string();
                        if method_items.len() < 2 {
                            continue;
                        }
                        let name = match &method_items[1] {
                            Edn::Symbol(s) => s.name.clone(),
                            _ => continue,
                        };
                        (name, Some(type_hint), 2)
                    }
                    Edn::Symbol(s) => (s.name.clone(), None, 1),
                    _ => continue,
                };

                // Parse arities with typed params
                let mut arities = Vec::new();
                for arity_item in &method_items[start_idx..] {
                    if let Edn::Vector(params) = arity_item {
                        let typed_params = Self::extract_typed_params(params);
                        arities.push(typed_params);
                    }
                }

                methods.push(analyze::AnalyzedProtocolMethod {
                    name: method_name,
                    arities,
                    return_type,
                });
            }
        }

        Ok(Some(analyze::AnalyzedProtocol { name, methods }))
    }

    /// Parse typed parameters from a vector [^type param ^type param ...]
    fn extract_typed_params(params: &[Edn]) -> Vec<analyze::ProtocolParam> {
        let mut result = Vec::new();
        let mut pending_type_hint: Option<String> = None;

        for item in params {
            if let Edn::Symbol(s) = item {
                if s.name.starts_with('^') {
                    pending_type_hint = Some(s.name[1..].to_string());
                } else {
                    result.push(analyze::ProtocolParam {
                        name: s.name.clone(),
                        type_hint: pending_type_hint.take(),
                    });
                }
            }
        }

        result
    }

    /// Extract an extend-type form into an AnalyzedExtension
    fn extract_extension(items: &[Edn]) -> CompileResult<Option<analyze::AnalyzedExtension>> {
        // (extend-type TypeName
        //   ProtocolName
        //   (method [args] body)
        //   ...)
        if items.len() < 2 {
            return Ok(None);
        }

        // Parse type name
        let type_name = match &items[1] {
            Edn::Symbol(s) => s.name.clone(),
            _ => return Ok(None),
        };

        // Parse protocol implementations
        let mut implementations = Vec::new();
        let mut current_protocol: Option<String> = None;
        let mut current_methods: Vec<analyze::AnalyzedMethodImpl> = Vec::new();

        for item in &items[2..] {
            match item {
                // Protocol name (bare symbol)
                Edn::Symbol(s) => {
                    // Save previous protocol if any
                    if let Some(protocol_name) = current_protocol.take() {
                        if !current_methods.is_empty() {
                            implementations.push(analyze::AnalyzedProtocolImpl {
                                protocol_name,
                                methods: std::mem::take(&mut current_methods),
                            });
                        }
                    }
                    current_protocol = Some(s.name.clone());
                }
                // Method implementation
                Edn::List(method_items) if !method_items.is_empty() => {
                    if current_protocol.is_none() {
                        continue;
                    }
                    // Parse method: (method-name [params] body...)
                    if let Some(Edn::Symbol(method_sym)) = method_items.first() {
                        if let Some(Edn::Vector(params_vec)) = method_items.get(1) {
                            let params: Vec<String> = params_vec
                                .iter()
                                .filter_map(|p| match p {
                                    Edn::Symbol(s) => Some(s.name.clone()),
                                    _ => None,
                                })
                                .collect();

                            // Body is remaining items wrapped in do if multiple
                            let body = if method_items.len() == 3 {
                                method_items[2].clone()
                            } else if method_items.len() > 3 {
                                Edn::List(
                                    std::iter::once(Edn::Symbol(suss_core::Symbol::new("do")))
                                        .chain(method_items[2..].iter().cloned())
                                        .collect(),
                                )
                            } else {
                                Edn::Nil
                            };

                            current_methods.push(analyze::AnalyzedMethodImpl {
                                name: method_sym.name.clone(),
                                params,
                                body,
                            });
                        }
                    }
                }
                _ => {}
            }
        }

        // Save final protocol
        if let Some(protocol_name) = current_protocol {
            if !current_methods.is_empty() {
                implementations.push(analyze::AnalyzedProtocolImpl {
                    protocol_name,
                    methods: current_methods,
                });
            }
        }

        Ok(Some(analyze::AnalyzedExtension {
            type_name,
            implementations,
        }))
    }

    /// Extract a deftype form into an AnalyzedDeftype
    fn extract_deftype(items: &[Edn]) -> CompileResult<Option<analyze::AnalyzedDeftype>> {
        // (deftype Name [fields...])
        // (deftype ^:type-id N Name [fields...])
        let mut idx = 1;
        let mut reserved_type_id = None;

        // Check for ^:type-id metadata
        if idx < items.len() {
            if let Edn::Symbol(sym) = &items[idx] {
                if sym.name == "^:type-id" {
                    idx += 1;
                    if idx >= items.len() {
                        return Err(CompileError::Parse("^:type-id requires a number".into()));
                    }
                    if let Edn::Number(n) = &items[idx] {
                        reserved_type_id = n.to_i64().map(|v| v as u32);
                    }
                    idx += 1;
                }
            }
        }

        // Parse type name
        let name = match items.get(idx) {
            Some(Edn::Symbol(s)) => s.name.clone(),
            _ => return Ok(None),
        };
        idx += 1;

        // Parse fields vector
        let fields = match items.get(idx) {
            Some(Edn::Vector(field_items)) => {
                let mut fields = Vec::new();
                let mut pending_type_hint = None;
                let mut pending_mutable = false;

                for item in field_items {
                    if let Edn::Symbol(sym) = item {
                        if sym.name.starts_with('^') {
                            let hint = &sym.name[1..];
                            if matches!(hint, "i32" | "i64" | "f64" | "eqref") {
                                pending_type_hint = Some(hint.to_string());
                            } else if hint == ":mutable" {
                                pending_mutable = true;
                            }
                        } else {
                            fields.push(analyze::DeftypeField {
                                name: sym.name.clone(),
                                type_hint: pending_type_hint.take(),
                                is_mutable: pending_mutable,
                            });
                            pending_mutable = false;
                        }
                    }
                }
                fields
            }
            _ => return Ok(None),
        };
        idx += 1;

        // Parse protocol implementations (same as extend-type)
        let mut implementations = Vec::new();
        let mut current_protocol: Option<String> = None;
        let mut current_methods: Vec<analyze::AnalyzedMethodImpl> = Vec::new();

        for item in &items[idx..] {
            match item {
                // Protocol name (bare symbol)
                Edn::Symbol(s) => {
                    // Save previous protocol if any
                    if let Some(protocol_name) = current_protocol.take() {
                        implementations.push(analyze::AnalyzedProtocolImpl {
                            protocol_name,
                            methods: std::mem::take(&mut current_methods),
                        });
                    }
                    current_protocol = Some(s.name.clone());
                }
                // Method implementation
                Edn::List(method_items) if !method_items.is_empty() => {
                    let method_name = match &method_items[0] {
                        Edn::Symbol(s) => s.name.clone(),
                        _ => continue,
                    };

                    if method_items.len() < 3 {
                        continue;
                    }

                    let params = match &method_items[1] {
                        Edn::Vector(p) => p.iter()
                            .filter_map(|x| if let Edn::Symbol(s) = x { Some(s.name.clone()) } else { None })
                            .collect(),
                        _ => continue,
                    };

                    let body = if method_items.len() == 3 {
                        method_items[2].clone()
                    } else {
                        Edn::List(
                            std::iter::once(Edn::Symbol(suss_core::Symbol::new("do")))
                                .chain(method_items[2..].iter().cloned())
                                .collect()
                        )
                    };

                    current_methods.push(analyze::AnalyzedMethodImpl {
                        name: method_name,
                        params,
                        body,
                    });
                }
                _ => {}
            }
        }

        // Save last protocol
        if let Some(protocol_name) = current_protocol {
            implementations.push(analyze::AnalyzedProtocolImpl {
                protocol_name,
                methods: current_methods,
            });
        }

        Ok(Some(analyze::AnalyzedDeftype {
            name,
            fields,
            implementations,
            reserved_type_id,
        }))
    }

    /// Extract a (def name (fn [params] body)) form into an AnalyzedFunction
    fn extract_def_fn(
        name: &str,
        is_exported: bool,
        fn_rest: &[Edn], // Items after 'fn': [params_vector, body...]
    ) -> CompileResult<Option<analyze::AnalyzedFunction>> {
        if fn_rest.is_empty() {
            return Ok(None);
        }

        // First item should be the params vector
        let params_vec = match &fn_rest[0] {
            Edn::Vector(v) => v,
            _ => return Ok(None),
        };

        // Parse params, handling & for variadic
        let mut params: Vec<(String, ir::Type)> = Vec::new();
        let mut rest_param: Option<String> = None;
        let mut found_amp = false;

        for p in params_vec {
            if let Edn::Symbol(s) = p {
                if s.name == "&" {
                    found_amp = true;
                } else if found_amp {
                    rest_param = Some(s.name.clone());
                    break;
                } else {
                    params.push((s.name.clone(), ir::Type::GcRef));
                }
            }
        }

        // Body is remaining items (after params vector)
        let body = if fn_rest.len() == 2 {
            fn_rest[1].clone()
        } else if fn_rest.len() > 2 {
            // Wrap multiple body expressions in do
            Edn::List(
                std::iter::once(Edn::Symbol(suss_core::Symbol::new("do")))
                    .chain(fn_rest[1..].iter().cloned())
                    .collect(),
            )
        } else {
            // No body - return nil
            Edn::Nil
        };

        Ok(Some(analyze::AnalyzedFunction {
            name: name.to_string(),
            exported: is_exported,
            export_name: if is_exported { Some(name.to_string()) } else { None },
            params,
            rest_param,
            return_type: ir::Type::GcRef,
            return_type_hint: None,
            docstring: None,
            body,
        }))
    }

    /// Compile expression as core WASM module (no WASI imports)
    fn compile_expr_core(
        &mut self,
        expr: suss_core::Edn,
        core_fns: Vec<analyze::AnalyzedFunction>,
        deftypes: Vec<analyze::AnalyzedDeftype>,
        protocols: Vec<analyze::AnalyzedProtocol>,
        extensions: Vec<analyze::AnalyzedExtension>,
    ) -> CompileResult<Vec<u8>> {
        // Infer the return type
        let return_type = analyze::infer_expr_type(&expr)?;

        // Create a synthetic analyzed module with core functions + eval
        let mut functions = core_fns;
        functions.push(analyze::AnalyzedFunction {
            name: "__eval".to_string(),
            exported: true,
            export_name: Some("eval".to_string()),
            params: Vec::new(),
            rest_param: None,
            return_type,
            return_type_hint: None,
            docstring: None,
            body: expr,
        });

        let analyzed = analyze::AnalyzedModule {
            namespace: None,
            world_target: None,
            imports: Vec::new(),
            suss_requires: Vec::new(),
            functions,
            globals: Vec::new(),
            protocols,
            extensions,
            deftypes,
        };

        // Lower to IR
        let ir = lower::lower(&analyzed)?;

        // Generate WASM (no WIT needed for expression compilation)
        codegen::generate_module(&ir)
    }

    /// Compile expression as WASM Component with WASI imports
    fn compile_expr_with_wasi(
        &mut self,
        expr: suss_core::Edn,
        wasi_calls: Vec<wasi::WasiFunctionInfo>,
        core_fns: Vec<analyze::AnalyzedFunction>,
        deftypes: Vec<analyze::AnalyzedDeftype>,
        protocols: Vec<analyze::AnalyzedProtocol>,
        extensions: Vec<analyze::AnalyzedExtension>,
    ) -> CompileResult<Vec<u8>> {
        // Infer the return type (may need to check WASI return types)
        let return_type = self.infer_expr_type_with_wasi(&expr, &wasi_calls)?;

        // Convert WASI calls to AnalyzedImports
        // Use "wasi.PACKAGE" as the alias so wasi.random/get-random-u64 maps to alias="wasi.random"
        let imports: Vec<analyze::AnalyzedImport> = wasi_calls
            .iter()
            .map(|info| {
                // Extract package from interface (e.g., "wasi:random/random@0.2.0" -> "random")
                let package = info
                    .wit_interface
                    .split(':')
                    .nth(1)
                    .and_then(|s| s.split('/').next())
                    .unwrap_or("unknown");

                analyze::AnalyzedImport {
                    alias: format!("wasi.{}", package),
                    wit_interface: info.wit_interface.clone(),
                    function_name: info.function_name.clone(),
                    params: info.params.clone(),
                    return_type: info.return_type.clone(),
                }
            })
            .collect();

        // Create a synthetic analyzed module with core functions + eval
        let mut functions = core_fns;
        functions.push(analyze::AnalyzedFunction {
            name: "__eval".to_string(),
            exported: true,
            export_name: Some("eval".to_string()),
            params: Vec::new(),
            rest_param: None,
            return_type,
            return_type_hint: None,
            docstring: None,
            body: expr,
        });

        let analyzed = analyze::AnalyzedModule {
            namespace: None,
            world_target: None,
            imports,
            suss_requires: Vec::new(),
            functions,
            globals: Vec::new(),
            protocols,
            extensions,
            deftypes,
        };

        // Lower to IR
        let ir = lower::lower(&analyzed)?;

        // Generate WASM Component with imports
        codegen::generate_component_with_imports(&ir)
    }

    /// Infer expression type, taking WASI return types into account
    fn infer_expr_type_with_wasi(
        &self,
        expr: &suss_core::Edn,
        wasi_calls: &[wasi::WasiFunctionInfo],
    ) -> CompileResult<ir::Type> {
        use suss_core::Edn;

        match expr {
            Edn::List(items) if !items.is_empty() => {
                if let Edn::Symbol(sym) = &items[0] {
                    // Check if it's a WASI call (handle namespaced symbols)
                    let full_name = if let Some(ns) = &sym.namespace {
                        format!("{}/{}", ns, sym.name)
                    } else {
                        sym.name.clone()
                    };
                    if let Some(info) = wasi::parse_wasi_call(&full_name) {
                        return Ok(info.return_type);
                    }
                    // Check standard forms
                    match sym.name.as_str() {
                        "if" => {
                            if items.len() >= 3 {
                                return self.infer_expr_type_with_wasi(&items[2], wasi_calls);
                            }
                        }
                        "do" => {
                            if items.len() > 1 {
                                return self
                                    .infer_expr_type_with_wasi(items.last().unwrap(), wasi_calls);
                            }
                        }
                        "let" => {
                            if items.len() > 2 {
                                return self
                                    .infer_expr_type_with_wasi(items.last().unwrap(), wasi_calls);
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }

        // Fall back to standard inference
        analyze::infer_expr_type(expr)
    }

    /// Compile Suss source code to a WASM component
    ///
    /// # Arguments
    ///
    /// * `source` - Suss source code
    /// * `wit_source` - WIT world definition
    ///
    /// # Returns
    ///
    /// The compiled WASM component bytes
    pub fn compile(&mut self, source: &str, wit_source: &str) -> CompileResult<Vec<u8>> {
        #[cfg(not(target_family = "wasm"))]
        {
            // Resolve the world before entering the effectful Macro phase.
            let mut resolve = Resolve::new();
            let package = resolve
                .push_str("world.wit", wit_source)
                .map_err(|error| CompileError::Wit(error.to_string()))?;
            let world = resolve
                .select_world(&[package], None)
                .map_err(|error| CompileError::Wit(error.to_string()))?;
            portable::aot::validate_boundary(&resolve, world)
                .map_err(|error| CompileError::Unsupported(error.to_string()))?;
            let fragments =
                portable_aot::prepare_source(source, None, &[std::path::PathBuf::from("src")])
                    .map_err(|error| CompileError::Semantic(error.to_string()))?;
            let mappings = portable::aot::source_export_mappings(&fragments, &resolve, world, &[])
                .map_err(|error| CompileError::ExportMismatch(error.to_string()))?;
            portable::aot::component(&fragments, &resolve, world, &mappings)
                .map_err(|error| CompileError::Component(error.to_string()))
        }
        #[cfg(target_family = "wasm")]
        {
            let _ = (source, wit_source);
            Err(CompileError::Unsupported(
                "Compiled source macro execution requires the native compiler host".into(),
            ))
        }
    }

    /// Compile a source file to its unambiguous selected WIT world using
    /// isolated compiled macros, preserving the original source filename.
    /// Dependencies resolve from `src`; Runtime initializers execute in the host.
    pub fn compile_files(&mut self, source_path: &str, wit_path: &str) -> CompileResult<Vec<u8>> {
        #[cfg(not(target_family = "wasm"))]
        {
            portable_aot::compile_file_typed(
                std::path::Path::new(source_path),
                std::path::Path::new(wit_path),
                None,
                &[std::path::PathBuf::from("src")],
                &[],
            )
        }
        #[cfg(target_family = "wasm")]
        {
            let _ = (source_path, wit_path);
            Err(CompileError::Unsupported(
                "Public file compilation requires the native compiled macro host".into(),
            ))
        }
    }

    /// Resolve an entry namespace and its phase dependencies from the configured
    /// source paths, compiling the selected WIT world without replaying sources.
    pub fn compile_with_namespaces(
        &mut self,
        entry_ns: &str,
        src_paths: &[std::path::PathBuf],
        wit_path: &str,
    ) -> CompileResult<Vec<u8>> {
        #[cfg(not(target_family = "wasm"))]
        {
            portable_aot::compile_namespace_typed(
                entry_ns,
                std::path::Path::new(wit_path),
                None,
                src_paths,
                &[],
            )
        }
        #[cfg(target_family = "wasm")]
        {
            let _ = (entry_ns, src_paths, wit_path);
            Err(CompileError::Unsupported(
                "Public namespace compilation requires the native compiled macro host".into(),
            ))
        }
    }

    /// Compile source containing the requested namespace and its `-main` var
    /// to the pinned official asynchronous WASI command profile.
    ///
    /// Native compilation uses the shared compiled Macro/Runtime pipeline and
    /// resolves dependencies from `src`. Runtime initializers execute only in
    /// the artifact host. Ordinary completion succeeds; explicit exit uses the
    /// command status policy.
    pub fn compile_for_main(&mut self, source: &str, main_ns: &str) -> CompileResult<Vec<u8>> {
        #[cfg(not(target_family = "wasm"))]
        {
            portable_aot::compile_main_source(
                source,
                None,
                main_ns,
                &[std::path::PathBuf::from("src")],
            )
            .map_err(CompileError::Semantic)
        }
        #[cfg(target_family = "wasm")]
        {
            let _ = (source, main_ns);
            Err(CompileError::Unsupported(
                "Public command compilation requires the native compiled macro host".into(),
            ))
        }
    }

    // ========================================================================
    // Namespace File Resolution
    // ========================================================================

    /// Convert a namespace name to a file path following Clojure conventions.
    ///
    /// Conversion rules:
    /// - Dots become directory separators: `myapp.core` → `myapp/core.sus`
    /// - Hyphens become underscores: `my-app.core` → `my_app/core.sus`
    ///
    /// # Arguments
    ///
    /// * `ns` - Namespace name (e.g., "myapp.utils", "my-app.core")
    /// * `src_paths` - List of source directories to search
    ///
    /// # Returns
    ///
    /// The first matching file path, or None if not found
    pub fn ns_to_path(ns: &str, src_paths: &[std::path::PathBuf]) -> Option<std::path::PathBuf> {
        // Convert dots to path separators, hyphens to underscores
        let path_str = ns.replace('.', "/").replace('-', "_");
        let file_name = format!("{}.sus", path_str);

        for src_path in src_paths {
            let candidate = src_path.join(&file_name);
            if candidate.exists() {
                return Some(candidate);
            }
        }
        None
    }

    /// Convert a file path back to a namespace name.
    ///
    /// This is the inverse of `ns_to_path`.
    ///
    /// # Arguments
    ///
    /// * `path` - File path relative to a src-path
    /// * `src_paths` - List of source directories
    ///
    /// # Returns
    ///
    /// The namespace name, or None if path doesn't match any src-path
    pub fn path_to_ns(path: &std::path::Path, src_paths: &[std::path::PathBuf]) -> Option<String> {
        for src_path in src_paths {
            if let Ok(relative) = path.strip_prefix(src_path) {
                // Remove .sus extension
                let without_ext = relative.with_extension("");
                // Convert path separators to dots, underscores to hyphens
                let ns = without_ext
                    .to_string_lossy()
                    .replace(std::path::MAIN_SEPARATOR, ".")
                    .replace('_', "-");
                return Some(ns);
            }
        }
        None
    }
}

// ============================================================================
// Dependency Resolution
// ============================================================================

use std::path::PathBuf;

/// Resolves dependencies between Suss namespaces for correct compilation order.
///
/// The resolver scans source files, extracts namespace declarations and requires,
/// builds a dependency graph, and produces a topological ordering for compilation.
#[derive(Debug, Default)]
pub struct DependencyResolver {
    /// Maps namespace name to its dependencies (required namespaces)
    deps: std::collections::HashMap<String, Vec<String>>,
    /// Maps namespace name to its parsed expressions
    parsed: std::collections::HashMap<String, Vec<Edn>>,
    /// Maps namespace name to its source file path
    files: std::collections::HashMap<String, PathBuf>,
    /// Source paths for namespace resolution
    src_paths: Vec<PathBuf>,
}

impl DependencyResolver {
    /// Create a new dependency resolver with the given source paths.
    pub fn new(src_paths: Vec<PathBuf>) -> Self {
        Self {
            deps: std::collections::HashMap::new(),
            parsed: std::collections::HashMap::new(),
            files: std::collections::HashMap::new(),
            src_paths,
        }
    }

    /// Scan a source file and extract namespace info.
    ///
    /// This performs a quick parse to extract the `(ns ...)` declaration
    /// and any `(require ...)` statements.
    pub fn scan_file(&mut self, path: &PathBuf) -> CompileResult<Option<String>> {
        let source = std::fs::read_to_string(path)
            .map_err(|e| CompileError::IoError(format!("Failed to read {}: {}", path.display(), e)))?;

        let mut parser_state = suss_reader::ParserState::new("suss");
        let exprs = suss_reader::parse_all(&source, &mut parser_state)
            .map_err(|e| CompileError::Parse(format!("Parse error in {}: {}", path.display(), e)))?;

        // Extract namespace name and dependencies from (ns ...) form
        let mut ns_name: Option<String> = None;
        let mut ns_deps: Vec<String> = Vec::new();

        for expr in &exprs {
            if let Edn::List(items) = expr {
                if let Some(Edn::Symbol(sym)) = items.first() {
                    if sym.name == "ns" {
                        // (ns myapp.core (require '[...]) ...)
                        if items.len() >= 2 {
                            if let Edn::Symbol(ns_sym) = &items[1] {
                                ns_name = Some(ns_sym.name.clone());
                            }
                        }
                        // Extract requires from ns form
                        for item in items.iter().skip(2) {
                            if let Edn::List(req_items) = item {
                                if let Some(Edn::Symbol(req_sym)) = req_items.first() {
                                    if req_sym.name == "require" {
                                        if let Some(dep) = self.extract_require_ns(req_items) {
                                            ns_deps.push(dep);
                                        }
                                    }
                                }
                            }
                        }
                    } else if sym.name == "require" {
                        // Top-level (require '[...])
                        if let Some(dep) = self.extract_require_ns(items) {
                            ns_deps.push(dep);
                        }
                    }
                }
            }
        }

        if let Some(ref name) = ns_name {
            self.deps.insert(name.clone(), ns_deps);
            self.parsed.insert(name.clone(), exprs);
            self.files.insert(name.clone(), path.clone());
        }

        Ok(ns_name)
    }

    /// Extract namespace name from a require form.
    ///
    /// Returns None for WASI requires (contain ':').
    fn extract_require_ns(&self, items: &[Edn]) -> Option<String> {
        // (require '[myapp.utils :as utils])
        if items.len() < 2 {
            return None;
        }

        // Get the spec - either quoted or unquoted vector
        let spec = match &items[1] {
            Edn::List(quote_items) if quote_items.len() == 2 => {
                if let Edn::Symbol(sym) = &quote_items[0] {
                    if sym.name == "quote" {
                        if let Edn::Vector(vec_items) = &quote_items[1] {
                            Some(vec_items)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            Edn::Vector(vec_items) => Some(vec_items),
            _ => None,
        }?;

        if spec.is_empty() {
            return None;
        }

        // First element is the namespace/interface name
        let name = match &spec[0] {
            Edn::Symbol(sym) => &sym.name,
            _ => return None,
        };

        // Skip WASI requires (contain ':')
        if name.contains(':') {
            return None;
        }

        Some(name.clone())
    }

    /// Scan starting from an entry namespace, discovering dependencies.
    ///
    /// This recursively discovers and scans all required namespaces.
    pub fn scan_from_entry(&mut self, entry_ns: &str) -> CompileResult<()> {
        let mut to_scan = vec![entry_ns.to_string()];
        let mut scanned = std::collections::HashSet::new();

        while let Some(ns) = to_scan.pop() {
            if scanned.contains(&ns) {
                continue;
            }
            scanned.insert(ns.clone());

            // Skip if already scanned
            if self.deps.contains_key(&ns) {
                // Add its dependencies to the scan queue
                if let Some(deps) = self.deps.get(&ns) {
                    for dep in deps {
                        if !scanned.contains(dep) {
                            to_scan.push(dep.clone());
                        }
                    }
                }
                continue;
            }

            // Find and scan the file
            if let Some(path) = Compiler::ns_to_path(&ns, &self.src_paths) {
                self.scan_file(&path)?;

                // Add discovered dependencies to scan queue
                if let Some(deps) = self.deps.get(&ns) {
                    for dep in deps {
                        if !scanned.contains(dep) {
                            to_scan.push(dep.clone());
                        }
                    }
                }
            } else {
                // Namespace not found - might be suss.core or external
                // We don't error here; missing deps are caught later
            }
        }

        Ok(())
    }

    /// Check for circular dependencies.
    ///
    /// Returns an error if a cycle is detected.
    pub fn check_cycles(&self) -> CompileResult<()> {
        use std::collections::HashSet;

        fn visit(
            ns: &str,
            deps: &std::collections::HashMap<String, Vec<String>>,
            visiting: &mut HashSet<String>,
            visited: &mut HashSet<String>,
            path: &mut Vec<String>,
        ) -> CompileResult<()> {
            if visited.contains(ns) {
                return Ok(());
            }
            if visiting.contains(ns) {
                path.push(ns.to_string());
                return Err(CompileError::CyclicDependency(path.join(" -> ")));
            }

            visiting.insert(ns.to_string());
            path.push(ns.to_string());

            if let Some(ns_deps) = deps.get(ns) {
                for dep in ns_deps {
                    visit(dep, deps, visiting, visited, path)?;
                }
            }

            path.pop();
            visiting.remove(ns);
            visited.insert(ns.to_string());
            Ok(())
        }

        let mut visiting = HashSet::new();
        let mut visited = HashSet::new();
        let mut path = Vec::new();

        for ns in self.deps.keys() {
            visit(ns, &self.deps, &mut visiting, &mut visited, &mut path)?;
        }

        Ok(())
    }

    /// Return namespaces in compilation order (dependencies first).
    ///
    /// Uses topological sort to ensure each namespace is compiled
    /// after all its dependencies.
    pub fn resolve_order(&self) -> CompileResult<Vec<String>> {
        self.check_cycles()?;

        use std::collections::HashSet;

        let mut result = Vec::new();
        let mut visited = HashSet::new();

        fn visit(
            ns: &str,
            deps: &std::collections::HashMap<String, Vec<String>>,
            visited: &mut HashSet<String>,
            result: &mut Vec<String>,
        ) {
            if visited.contains(ns) {
                return;
            }
            visited.insert(ns.to_string());

            // Visit dependencies first
            if let Some(ns_deps) = deps.get(ns) {
                for dep in ns_deps {
                    visit(dep, deps, visited, result);
                }
            }

            result.push(ns.to_string());
        }

        // Visit all namespaces
        for ns in self.deps.keys() {
            visit(ns, &self.deps, &mut visited, &mut result);
        }

        Ok(result)
    }

    /// Get parsed expressions for a namespace.
    pub fn get_parsed(&self, ns: &str) -> Option<&Vec<Edn>> {
        self.parsed.get(ns)
    }

    /// Get source file path for a namespace.
    pub fn get_file(&self, ns: &str) -> Option<&PathBuf> {
        self.files.get(ns)
    }

    /// Get all discovered namespaces.
    pub fn namespaces(&self) -> impl Iterator<Item = &String> {
        self.deps.keys()
    }
}

impl Compiler {
    /// Compile the configured selected worlds through isolated compiled phases.
    /// Namespace entries, selected WIT worlds and explicit export mappings share
    /// the CLI project rules. Return all artifacts without publishing output.
    pub fn compile_project(
        &mut self,
        config: &SussConfig,
        world: Option<&str>,
    ) -> CompileResult<std::collections::HashMap<String, Vec<u8>>> {
        #[cfg(not(target_family = "wasm"))]
        {
            portable_project::compile_project_typed(config, world)
                .map(|artifacts| artifacts.into_iter().collect())
        }
        #[cfg(target_family = "wasm")]
        {
            let _ = (config, world);
            Err(CompileError::Unsupported(
                "Public project compilation requires the native compiled macro host".into(),
            ))
        }
    }
}

impl Default for Compiler {
    fn default() -> Self {
        Self::new()
    }
}

/// Export load_core_exprs for external use (e.g., symbol extraction)
pub fn load_core_exprs_public() -> CompileResult<Vec<Edn>> {
    load_core_exprs()
}

#[cfg(test)]
mod deftype_tests {
    use super::*;

    #[test]
    fn test_deftype_basic() {
        let mut compiler = Compiler::new();
        let source = "(deftype Point [x y]) 42";
        let result = compiler.compile_expr(source);
        if let Err(e) = &result {
            eprintln!("Compilation error: {:?}", e);
        }
        assert!(result.is_ok(), "Should compile successfully");

        let wasm = result.unwrap();
        std::fs::write("/tmp/deftype_test.wasm", &wasm).unwrap();
    }

    #[test]
    fn test_deftype_constructor() {
        let mut compiler = Compiler::new();
        let source = "(deftype Point [x y]) (->Point 10 20)";
        let result = compiler.compile_expr(source);
        if let Err(e) = &result {
            eprintln!("Compilation error: {:?}", e);
        }
        assert!(result.is_ok(), "Should compile successfully");

        let wasm = result.unwrap();
        std::fs::write("/tmp/deftype_constructor.wasm", &wasm).unwrap();
    }

    #[test]
    fn test_deftype_field_access() {
        let mut compiler = Compiler::new();
        let source = "(deftype Point [x y]) (.-x (->Point 10 20))";
        let result = compiler.compile_expr(source);
        if let Err(e) = &result {
            eprintln!("Compilation error: {:?}", e);
        }
        assert!(result.is_ok(), "Should compile successfully");

        let wasm = result.unwrap();
        std::fs::write("/tmp/deftype_field.wasm", &wasm).unwrap();
    }

    #[test]
    fn test_deftype_typed_i32_fields() {
        let mut compiler = Compiler::new();
        let source = "(deftype Vec2 [^i32 x ^i32 y]) (+ (.-x (->Vec2 100 200)) (.-y (->Vec2 100 200)))";
        let result = compiler.compile_expr(source);
        if let Err(e) = &result {
            eprintln!("Compilation error: {:?}", e);
        }
        assert!(result.is_ok(), "Should compile typed i32 fields");

        let wasm = result.unwrap();
        std::fs::write("/tmp/deftype_typed_i32.wasm", &wasm).unwrap();
    }

    #[test]
    fn test_deftype_typed_f64_field() {
        let mut compiler = Compiler::new();
        let source = "(deftype Floaty [^f64 val]) (.-val (->Floaty 3.14))";
        let result = compiler.compile_expr(source);
        if let Err(e) = &result {
            eprintln!("Compilation error: {:?}", e);
        }
        assert!(result.is_ok(), "Should compile typed f64 field");

        let wasm = result.unwrap();
        std::fs::write("/tmp/deftype_typed_f64.wasm", &wasm).unwrap();
    }

    #[test]
    fn test_deftype_mixed_fields() {
        let mut compiler = Compiler::new();
        // Mixed: i32 typed field + eqref (default) field
        let source = r#"(deftype Mixed [^i32 count name]) (.-count (->Mixed 42 "test"))"#;
        let result = compiler.compile_expr(source);
        if let Err(e) = &result {
            eprintln!("Compilation error: {:?}", e);
        }
        assert!(result.is_ok(), "Should compile mixed field types");

        let wasm = result.unwrap();
        std::fs::write("/tmp/deftype_mixed.wasm", &wasm).unwrap();
    }

    #[test]
    fn test_deftype_with_protocol() {
        let mut compiler = Compiler::new();
        // Define a type with protocol implementation
        let source = r#"
            (deftype Counter [^i32 value]
              ICounted
              (-count [this] (.-value this)))
            (-count (->Counter 42))
        "#;
        let result = compiler.compile_expr(source);
        if let Err(e) = &result {
            eprintln!("Compilation error: {:?}", e);
        }
        assert!(result.is_ok(), "Should compile deftype with protocol");

        let wasm = result.unwrap();
        std::fs::write("/tmp/deftype_protocol.wasm", &wasm).unwrap();
    }

    #[test]
    fn test_deftype_reserved_type_id() {
        let mut compiler = Compiler::new();
        // Use reserved type ID (for core.sus bootstrap types)
        let source = "(deftype ^:type-id 39 CustomNode [data]) (instance? CustomNode (->CustomNode 42))";
        let result = compiler.compile_expr(source);
        if let Err(e) = &result {
            eprintln!("Compilation error: {:?}", e);
        }
        assert!(result.is_ok(), "Should compile deftype with reserved type-id");

        let wasm = result.unwrap();
        std::fs::write("/tmp/deftype_reserved.wasm", &wasm).unwrap();
    }

    #[test]
    fn dump_apply_wasm() {
        let source = "(apply + [1 2])";
        let mut compiler = Compiler::new();
        match compiler.compile_expr(source) {
            Ok(wasm) => {
                std::fs::write("/tmp/apply_debug.wasm", &wasm).unwrap();
                eprintln!("WASM written to /tmp/apply_debug.wasm");
            }
            Err(e) => eprintln!("Compile error: {:?}", e),
        }
    }

    #[test]
    fn test_deftype_mutable_field() {
        let mut compiler = Compiler::new();
        let source = r#"
            (deftype MutableBox [^:mutable val])
            (let [b (->MutableBox 10)]
              (set! (.-val b) 99)
              (.-val b))
        "#;
        let result = compiler.compile_expr(source);
        if let Err(e) = &result {
            eprintln!("Compilation error: {:?}", e);
        }
        assert!(result.is_ok(), "Should compile mutable field set");

        let wasm = result.unwrap();
        std::fs::write("/tmp/deftype_mutable.wasm", &wasm).unwrap();
    }
}
