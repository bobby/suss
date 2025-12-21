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
mod error;

pub use error::{CompileError, CompileResult};

use suss_core::Interner;
use suss_reader::ParserState;
use wit_parser::Resolve;

/// The Suss static compiler
pub struct Compiler {
    /// Shared interner for symbol/keyword names
    interner: Interner,
}

impl Compiler {
    /// Create a new compiler instance
    pub fn new() -> Self {
        Self {
            interner: Interner::new(),
        }
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
        parser_state.interner = std::mem::take(&mut self.interner);

        let exprs = suss_reader::parse_all_and_intern(source, &mut parser_state)
            .map_err(|e| CompileError::Parse(e.to_string()))?;

        self.interner = parser_state.interner;

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
        let module = analyze::analyze(&exprs, &self.interner, &resolve, *world_id)?;

        // Lower to IR
        let ir = lower::lower(&module, &self.interner)?;

        // Generate WASM
        let wasm = codegen::generate(&ir, &resolve, *world_id)?;

        Ok(wasm)
    }

    /// Compile from file paths
    ///
    /// This method handles WIT deps automatically by looking for a `deps` folder
    /// in the same directory as the WIT file.
    pub fn compile_files(&mut self, source_path: &str, wit_path: &str) -> CompileResult<Vec<u8>> {
        use std::path::Path;

        let source = std::fs::read_to_string(source_path)
            .map_err(|e| CompileError::Io(format!("Failed to read {}: {}", source_path, e)))?;

        // Parse the Suss source
        let mut parser_state = ParserState::new("suss");
        parser_state.interner = std::mem::take(&mut self.interner);

        let exprs = suss_reader::parse_all_and_intern(&source, &mut parser_state)
            .map_err(|e| CompileError::Parse(e.to_string()))?;

        self.interner = parser_state.interner;

        // Parse WIT - check for deps directory first
        let wit_path = Path::new(wit_path);
        let mut resolve = Resolve::new();

        // Check if there's a deps folder next to the WIT file
        if let Some(parent) = wit_path.parent() {
            let deps_dir = parent.join("deps");
            if deps_dir.is_dir() {
                // Push each package in deps first
                for entry in std::fs::read_dir(&deps_dir)
                    .map_err(|e| CompileError::Io(format!("Failed to read deps: {}", e)))?
                {
                    let entry = entry.map_err(|e| CompileError::Io(format!("Failed to read entry: {}", e)))?;
                    let path = entry.path();
                    if path.is_dir() {
                        // Push the package directory
                        let _ = resolve.push_path(&path);
                    }
                }
            }
        }

        // Push the main WIT file
        let (pkg_id, _source_map) = resolve
            .push_path(wit_path)
            .map_err(|e| CompileError::Wit(e.to_string()))?;

        // Get the world from the package
        let pkg = &resolve.packages[pkg_id];
        let world_id = pkg.worlds.values().next()
            .ok_or_else(|| CompileError::Wit("No world found in WIT file".to_string()))?;

        // Analyze the source
        let module = analyze::analyze(&exprs, &self.interner, &resolve, *world_id)?;

        // Lower to IR
        let ir = lower::lower(&module, &self.interner)?;

        // Generate WASM
        let wasm = codegen::generate(&ir, &resolve, *world_id)?;

        Ok(wasm)
    }
}

impl Default for Compiler {
    fn default() -> Self {
        Self::new()
    }
}
