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

mod config;
mod error;
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

use suss_core::Edn;
use suss_reader::ParserState;
use wit_parser::Resolve;

/// The Suss static compiler: expression artifacts and AOT components, all
/// through the compiled Macro/Runtime pipeline and the shipped bootstrap.
pub struct Compiler {}

impl Compiler {
    /// Prepare host-owned ABI2 expression artifacts through isolated compiled
    /// macros. Runtime initializers run only when the returned artifact executes.
    #[cfg(not(target_family = "wasm"))]
    pub fn prepare_expression(
        &mut self,
        source: &str,
        source_paths: &[std::path::PathBuf],
    ) -> CompileResult<portable_expression::ExpressionArtifact> {
        portable_expression::prepare(source, source_paths)
            .map_err(|error| CompileError::Semantic(error.to_string()))
    }
    /// Create a new compiler instance.
    pub fn new() -> Self {
        Self {}
    }

    /// Compile an expression bundle using the cached compiled core bootstrap.
    /// User macros execute in an isolated compiled phase; Runtime initialization
    /// is deferred. This is a fresh compilation, not a source-replaying REPL.
    #[cfg(not(target_family = "wasm"))]
    pub fn compile_expr_cached(&mut self, expr_source: &str) -> CompileResult<CompiledExpr> {
        self.compile_expr_with_info(expr_source)
    }

    /// Prepare a native expression through the common compiled macro pipeline.
    /// Runtime initializers are deferred until `CompiledExpr::execute`; the
    /// complete bundle, not its entry-module inspection copy, is executable.
    /// Each call has an isolated Macro session and fresh user compilation. The
    /// reproducible compiled core bootstrap is shared with the cached entrypoint.
    #[cfg(not(target_family = "wasm"))]
    pub fn compile_expr_with_info(&mut self, expr_source: &str) -> CompileResult<CompiledExpr> {
        let prepared = self.prepare_expression(expr_source, &[std::path::PathBuf::from("src")])?;
        let wasm = prepared.modules().last().expect("expression bootstrap module").to_vec();
        Ok(CompiledExpr { wasm, is_component: false, prepared: Some(prepared) })
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

#[cfg(test)]
mod deftype_tests {
    use super::*;

    #[test]
    fn test_deftype_basic() {
        let mut compiler = Compiler::new();
        let source = "(deftype Point [x y]) 42";
        let result = compiler.compile_expr_with_info(source);
        if let Err(e) = &result {
            eprintln!("Compilation error: {:?}", e);
        }
        assert!(result.is_ok(), "Should compile successfully");
    }

    #[test]
    fn test_deftype_constructor() {
        let mut compiler = Compiler::new();
        let source = "(deftype Point [x y]) (->Point 10 20)";
        let result = compiler.compile_expr_with_info(source);
        if let Err(e) = &result {
            eprintln!("Compilation error: {:?}", e);
        }
        assert!(result.is_ok(), "Should compile successfully");
    }

    #[test]
    fn test_deftype_field_access() {
        let mut compiler = Compiler::new();
        let source = "(deftype Point [x y]) (.-x (->Point 10 20))";
        let result = compiler.compile_expr_with_info(source);
        if let Err(e) = &result {
            eprintln!("Compilation error: {:?}", e);
        }
        assert!(result.is_ok(), "Should compile successfully");
    }

    #[test]
    fn test_deftype_typed_i32_fields() {
        let mut compiler = Compiler::new();
        let source = "(deftype Vec2 [^i32 x ^i32 y]) (+ (.-x (->Vec2 100 200)) (.-y (->Vec2 100 200)))";
        let result = compiler.compile_expr_with_info(source);
        if let Err(e) = &result {
            eprintln!("Compilation error: {:?}", e);
        }
        assert!(result.is_ok(), "Should compile typed i32 fields");
    }

    #[test]
    fn test_deftype_typed_f64_field() {
        let mut compiler = Compiler::new();
        let source = "(deftype Floaty [^f64 val]) (.-val (->Floaty 3.14))";
        let result = compiler.compile_expr_with_info(source);
        if let Err(e) = &result {
            eprintln!("Compilation error: {:?}", e);
        }
        assert!(result.is_ok(), "Should compile typed f64 field");
    }

    #[test]
    fn test_deftype_mixed_fields() {
        let mut compiler = Compiler::new();
        // Mixed: i32 typed field + eqref (default) field
        let source = r#"(deftype Mixed [^i32 count name]) (.-count (->Mixed 42 "test"))"#;
        let result = compiler.compile_expr_with_info(source);
        if let Err(e) = &result {
            eprintln!("Compilation error: {:?}", e);
        }
        assert!(result.is_ok(), "Should compile mixed field types");
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
        let result = compiler.compile_expr_with_info(source);
        if let Err(e) = &result {
            eprintln!("Compilation error: {:?}", e);
        }
        assert!(result.is_ok(), "Should compile deftype with protocol");
    }

    #[test]
    fn test_deftype_rejects_metadata_on_a_number() {
        let mut compiler = Compiler::new();
        // The prototype's `^:type-id N` reserved-id convention is not part of
        // the design; metadata cannot apply to a number (design section 4).
        let source = "(deftype ^:type-id 39 CustomNode [data]) (instance? CustomNode (->CustomNode 42))";
        let error = match compiler.compile_expr_with_info(source) {
            Ok(_) => panic!("metadata on a number literal must not compile"),
            Err(error) => error.to_string(),
        };
        assert!(
            error.contains("Metadata requires a symbol or collection at bytes 19..21"),
            "unexpected diagnostic: {error}"
        );
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
        let result = compiler.compile_expr_with_info(source);
        if let Err(e) = &result {
            eprintln!("Compilation error: {:?}", e);
        }
        assert!(result.is_ok(), "Should compile mutable field set");
    }
}
