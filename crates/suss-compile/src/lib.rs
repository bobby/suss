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

mod ir;
mod analyze;
mod lower;
mod codegen;
mod component;
mod config;
mod error;
mod wasi;

pub use config::{SussConfig, WorldConfig};
pub use error::{CompileError, CompileResult};

/// Bundled WASI version
pub const BUNDLED_WASI_VERSION: &str = wasi::WASI_VERSION;

use suss_reader::ParserState;
use wit_parser::Resolve;

/// The Suss static compiler
pub struct Compiler;

impl Compiler {
    /// Create a new compiler instance
    pub fn new() -> Self {
        Self
    }

    /// Compile a single expression to a WASM module
    ///
    /// This creates a minimal WASM module with a single exported function `eval`
    /// that returns the result of the expression. No WIT file is required.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let compiler = Compiler::new();
    /// let wasm = compiler.compile_expr("(+ 1 2)")?;
    /// // Run with wasmtime, call `eval` function, get result
    /// ```
    pub fn compile_expr(&mut self, expr_source: &str) -> CompileResult<Vec<u8>> {
        // Parse the expression
        let mut parser_state = ParserState::new("suss");
        let expr = suss_reader::parse(expr_source, &mut parser_state)
            .map_err(|e| CompileError::Parse(e.to_string()))?;

        // Infer the return type
        let return_type = analyze::infer_expr_type(&expr)?;

        // Create a synthetic analyzed module with one function
        let analyzed = analyze::AnalyzedModule {
            namespace: None,
            world_target: None,
            imports: Vec::new(),
            functions: vec![analyze::AnalyzedFunction {
                name: "__eval".to_string(),
                exported: true,
                export_name: Some("eval".to_string()),
                params: Vec::new(),
                return_type,
                body: expr,
            }],
            globals: Vec::new(),
        };

        // Lower to IR
        let ir = lower::lower(&analyzed)?;

        // Generate WASM (no WIT needed for expression compilation)
        let wasm = codegen::generate_module(&ir)?;

        Ok(wasm)
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
        // Parse the Suss source
        let mut parser_state = ParserState::new("suss");
        let exprs = suss_reader::parse_all(source, &mut parser_state)
            .map_err(|e| CompileError::Parse(e.to_string()))?;

        // Parse the WIT definition
        let mut resolve = Resolve::new();
        let pkg_id = resolve
            .push_str("world.wit", wit_source)
            .map_err(|e| CompileError::Wit(e.to_string()))?;

        // Get the world from the package
        let pkg = &resolve.packages[pkg_id];
        let world_id = pkg.worlds.values().next()
            .ok_or_else(|| CompileError::Wit("No world found in WIT file".to_string()))?;

        // Analyze the source
        let module = analyze::analyze(&exprs, &resolve, *world_id)?;

        // Lower to IR
        let ir = lower::lower(&module)?;

        // Generate core WASM module
        let core_wasm = codegen::generate(&ir, &resolve, *world_id)?;

        // Wrap as WASM Component
        component::encode_component(&core_wasm, &resolve, *world_id)
    }

    /// Compile from file paths
    ///
    /// This method automatically loads bundled WASI definitions when it detects
    /// `wasi:*` imports in the world.wit file. Users don't need to set up a deps/
    /// folder for WASI packages.
    ///
    /// User-provided deps/ folders are still supported and take precedence over
    /// bundled WASI for custom packages.
    pub fn compile_files(&mut self, source_path: &str, wit_path: &str) -> CompileResult<Vec<u8>> {
        use std::path::Path;

        let source = std::fs::read_to_string(source_path)
            .map_err(|e| CompileError::Io(format!("Failed to read {}: {}", source_path, e)))?;

        // Read the WIT file content for WASI detection
        let wit_source = std::fs::read_to_string(wit_path)
            .map_err(|e| CompileError::Io(format!("Failed to read {}: {}", wit_path, e)))?;

        // Parse the Suss source
        let mut parser_state = ParserState::new("suss");
        let exprs = suss_reader::parse_all(&source, &mut parser_state)
            .map_err(|e| CompileError::Parse(e.to_string()))?;

        let wit_path = Path::new(wit_path);
        let mut resolve = Resolve::new();

        // Auto-detect and load bundled WASI packages
        let needed_wasi = wasi::detect_needed_packages(&wit_source);
        for pkg_name in &needed_wasi {
            if let Some(combined) = wasi::get_combined_package(pkg_name) {
                let _ = resolve.push_str(&format!("wasi-{}.wit", pkg_name), &combined);
            }
        }

        // Check if there's a user-provided deps folder (for custom packages)
        if let Some(parent) = wit_path.parent() {
            let deps_dir = parent.join("deps");
            if deps_dir.is_dir() {
                for entry in std::fs::read_dir(&deps_dir)
                    .map_err(|e| CompileError::Io(format!("Failed to read deps: {}", e)))?
                {
                    let entry = entry.map_err(|e| CompileError::Io(format!("Failed to read entry: {}", e)))?;
                    let path = entry.path();
                    if path.is_dir() {
                        let _ = resolve.push_path(&path);
                    }
                }
            }
        }

        // Push the main WIT file as a string (since we already read it)
        let pkg_id = resolve
            .push_str(wit_path.to_string_lossy().as_ref(), &wit_source)
            .map_err(|e| CompileError::Wit(e.to_string()))?;

        // Get the world from the package
        let pkg = &resolve.packages[pkg_id];
        let world_id = pkg.worlds.values().next()
            .ok_or_else(|| CompileError::Wit("No world found in WIT file".to_string()))?;

        // Analyze the source
        let module = analyze::analyze(&exprs, &resolve, *world_id)?;

        // Lower to IR
        let ir = lower::lower(&module)?;

        // Generate core WASM module
        let core_wasm = codegen::generate(&ir, &resolve, *world_id)?;

        // Wrap as WASM Component
        component::encode_component(&core_wasm, &resolve, *world_id)
    }

    /// Compile a project from deps.suss configuration
    ///
    /// This method reads deps.suss, scans source paths, and compiles each world
    /// defined in the configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - Loaded project configuration from deps.suss
    /// * `world` - Optional specific world to compile (if None, compiles all worlds)
    ///
    /// # Returns
    ///
    /// A map of world names to their compiled WASM component bytes
    pub fn compile_project(
        &mut self,
        config: &SussConfig,
        world: Option<&str>,
    ) -> CompileResult<std::collections::HashMap<String, Vec<u8>>> {
        use std::collections::HashMap;

        // Collect all source files from src-paths
        let source_files = self.collect_source_files(config)?;

        // Parse all source files and group by world target
        let sources_by_world = self.group_sources_by_world(&source_files)?;

        // Determine which worlds to compile
        let worlds_to_compile: Vec<&str> = match world {
            Some(w) => {
                if config.worlds.contains_key(w) {
                    vec![w]
                } else {
                    return Err(CompileError::Config(format!(
                        "World '{}' not found in deps.suss. Available: {:?}",
                        w,
                        config.worlds.keys().collect::<Vec<_>>()
                    )));
                }
            }
            None => config.worlds.keys().map(|s| s.as_str()).collect(),
        };

        let mut results = HashMap::new();

        for world_name in worlds_to_compile {
            let world_config = &config.worlds[world_name];

            // Get sources for this world
            let world_sources = sources_by_world
                .get(world_name)
                .cloned()
                .unwrap_or_default();

            if world_sources.is_empty() {
                return Err(CompileError::Config(format!(
                    "No source files found with (gen-world {}) for world '{}'",
                    world_name, world_name
                )));
            }

            // Compile this world
            let wasm = self.compile_world(
                &world_sources,
                &config.wit_path(world_name)?,
            )?;

            results.insert(world_name.to_string(), wasm);
        }

        Ok(results)
    }

    /// Compile a single world from source files
    fn compile_world(
        &mut self,
        source_files: &[std::path::PathBuf],
        wit_path: &std::path::Path,
    ) -> CompileResult<Vec<u8>> {
        use wit_parser::Resolve;

        // Read and parse all source files
        let mut all_exprs = Vec::new();
        for path in source_files {
            let source = std::fs::read_to_string(path)
                .map_err(|e| CompileError::Io(format!("Failed to read {}: {}", path.display(), e)))?;

            let mut parser_state = ParserState::new("suss");
            let exprs = suss_reader::parse_all(&source, &mut parser_state)
                .map_err(|e| CompileError::Parse(e.to_string()))?;
            all_exprs.extend(exprs);
        }

        // Read WIT file
        let wit_source = std::fs::read_to_string(wit_path)
            .map_err(|e| CompileError::Io(format!("Failed to read {}: {}", wit_path.display(), e)))?;

        // Parse WIT
        let mut resolve = Resolve::new();

        // Auto-detect and load bundled WASI packages
        let needed_wasi = wasi::detect_needed_packages(&wit_source);
        for pkg_name in &needed_wasi {
            if let Some(combined) = wasi::get_combined_package(pkg_name) {
                let _ = resolve.push_str(&format!("wasi-{}.wit", pkg_name), &combined);
            }
        }

        // Check for user-provided deps folder
        if let Some(parent) = wit_path.parent() {
            let deps_dir = parent.join("deps");
            if deps_dir.is_dir() {
                for entry in std::fs::read_dir(&deps_dir)
                    .map_err(|e| CompileError::Io(format!("Failed to read deps: {}", e)))?
                {
                    let entry = entry.map_err(|e| CompileError::Io(format!("Failed to read entry: {}", e)))?;
                    let path = entry.path();
                    if path.is_dir() {
                        let _ = resolve.push_path(&path);
                    }
                }
            }
        }

        // Push the main WIT file
        let pkg_id = resolve
            .push_str(wit_path.to_string_lossy().as_ref(), &wit_source)
            .map_err(|e| CompileError::Wit(e.to_string()))?;

        // Get the world from the package
        let pkg = &resolve.packages[pkg_id];
        let world_id = pkg.worlds.values().next()
            .ok_or_else(|| CompileError::Wit("No world found in WIT file".to_string()))?;

        // Analyze the source
        let module = analyze::analyze(&all_exprs, &resolve, *world_id)?;

        // Lower to IR
        let ir = lower::lower(&module)?;

        // Generate core WASM module
        let core_wasm = codegen::generate(&ir, &resolve, *world_id)?;

        // Wrap as WASM Component
        component::encode_component(&core_wasm, &resolve, *world_id)
    }

    /// Collect all .suss source files from configured src-paths
    fn collect_source_files(&self, config: &SussConfig) -> CompileResult<Vec<std::path::PathBuf>> {
        let mut files = Vec::new();

        for src_path in &config.src_paths {
            if !src_path.exists() {
                continue;
            }

            self.collect_suss_files_recursive(src_path, &mut files)?;
        }

        Ok(files)
    }

    fn collect_suss_files_recursive(
        &self,
        dir: &std::path::Path,
        files: &mut Vec<std::path::PathBuf>,
    ) -> CompileResult<()> {
        if dir.is_file() {
            if dir.extension().and_then(|e| e.to_str()) == Some("suss") {
                files.push(dir.to_path_buf());
            }
            return Ok(());
        }

        for entry in std::fs::read_dir(dir)
            .map_err(|e| CompileError::Io(format!("Failed to read {}: {}", dir.display(), e)))?
        {
            let entry = entry.map_err(|e| CompileError::Io(e.to_string()))?;
            let path = entry.path();

            if path.is_dir() {
                self.collect_suss_files_recursive(&path, files)?;
            } else if path.extension().and_then(|e| e.to_str()) == Some("suss") {
                files.push(path);
            }
        }

        Ok(())
    }

    /// Group source files by their world_target
    fn group_sources_by_world(
        &self,
        files: &[std::path::PathBuf],
    ) -> CompileResult<std::collections::HashMap<String, Vec<std::path::PathBuf>>> {
        use std::collections::HashMap;

        let mut result: HashMap<String, Vec<std::path::PathBuf>> = HashMap::new();

        for path in files {
            let source = std::fs::read_to_string(path)
                .map_err(|e| CompileError::Io(format!("Failed to read {}: {}", path.display(), e)))?;

            // Quick parse to find world_target
            let world_target = self.extract_world_target(&source)?;

            if let Some(world) = world_target {
                result.entry(world).or_default().push(path.clone());
            }
        }

        Ok(result)
    }

    /// Extract world_target from source without full analysis
    fn extract_world_target(&self, source: &str) -> CompileResult<Option<String>> {
        let mut parser_state = ParserState::new("suss");
        let exprs = suss_reader::parse_all(source, &mut parser_state)
            .map_err(|e| CompileError::Parse(e.to_string()))?;

        for expr in exprs {
            if let suss_core::Edn::List(items) = expr {
                if items.is_empty() {
                    continue;
                }
                if let suss_core::Edn::Symbol(sym) = &items[0] {
                    if sym.name == "ns" {
                        // Look for (gen-world :world-name) clause
                        for item in &items[2..] {
                            if let suss_core::Edn::List(clause) = item {
                                if clause.is_empty() {
                                    continue;
                                }
                                if let suss_core::Edn::Symbol(clause_sym) = &clause[0] {
                                    if clause_sym.name == "gen-world" && clause.len() >= 2 {
                                        if let suss_core::Edn::Keyword(kw) = &clause[1] {
                                            return Ok(Some(kw.to_string()));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(None)
    }
}

impl Default for Compiler {
    fn default() -> Self {
        Self::new()
    }
}
