//! Session state management for the Suss REPL
//!
//! Provides enhanced REPL state with:
//! - WASM compilation caching (hash-based)
//! - Core.sus preloading
//! - Symbol table for tab completion
//! - Compiler instance reuse

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

use suss_compile::{CompiledExpr, Compiler};

/// Symbol kind for tab completion display
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SymbolKind {
    Function,
    Macro,
    Var,
    Protocol,
    Type,
    Special,
}

/// Symbol entry for tab completion
#[derive(Debug, Clone)]
pub struct SymbolEntry {
    pub name: String,
    pub kind: SymbolKind,
    pub namespace: Option<String>,
    pub arity: Option<usize>,
}

/// Tracks a loaded namespace file for hot reload
pub struct LoadedNamespace {
    pub path: PathBuf,
    pub mtime: SystemTime,
    pub source: String,
}

/// Maximum entries in the WASM cache
const MAX_CACHE_ENTRIES: usize = 100;

/// Session state with compilation caching
pub struct SessionState {
    // === Namespace Management ===
    /// Accumulated definitions per namespace: namespace -> (def-name -> source)
    pub ns_definitions: HashMap<String, HashMap<String, String>>,
    /// Current namespace context
    pub current_ns: String,
    /// Namespace aliases from requires: alias -> full namespace name
    pub ns_aliases: HashMap<String, String>,
    /// Loaded namespace files: namespace -> loaded info (path, mtime, source)
    pub loaded_namespaces: HashMap<String, LoadedNamespace>,
    /// Source paths for namespace resolution
    pub src_paths: Vec<PathBuf>,

    // === Compilation Caching ===
    /// Compiler instance (holds cached core.sus)
    compiler: Compiler,
    /// WASM cache: source_hash -> compiled bytes
    wasm_cache: HashMap<u64, CacheEntry>,
    /// LRU order for cache eviction
    cache_order: Vec<u64>,
    /// Cache statistics
    pub cache_hits: u64,
    pub cache_misses: u64,

    // === Symbol Table ===
    /// Symbols for tab completion
    pub symbols: HashMap<String, SymbolEntry>,
    /// Core symbols (never removed)
    pub core_symbol_names: std::collections::HashSet<String>,
}

/// A cached compilation entry
struct CacheEntry {
    compiled: Arc<CompiledExpr>,
}

impl SessionState {
    /// Create a new session state
    pub fn new() -> Self {
        let mut state = Self {
            ns_definitions: HashMap::new(),
            current_ns: "user".to_string(),
            ns_aliases: HashMap::new(),
            loaded_namespaces: HashMap::new(),
            src_paths: vec![PathBuf::from("src")],
            compiler: Compiler::new(),
            wasm_cache: HashMap::new(),
            cache_order: Vec::new(),
            cache_hits: 0,
            cache_misses: 0,
            symbols: HashMap::new(),
            core_symbol_names: std::collections::HashSet::new(),
        };

        // Initialize builtin symbols
        state.init_builtin_symbols();

        state
    }

    /// Initialize builtin symbols (special forms, core functions)
    fn init_builtin_symbols(&mut self) {
        // Special forms
        let special_forms = [
            "if", "do", "let", "fn", "loop", "recur", "quote", "def", "defn",
            "defmacro", "deftype", "defprotocol", "extend-type", "ns", "require",
        ];
        for name in special_forms {
            self.add_symbol(name, SymbolKind::Special, None);
            self.core_symbol_names.insert(name.to_string());
        }

        // Core macros
        let macros = [
            "when", "when-not", "when-let", "if-let", "and", "or", "cond", "case",
            "->", "->>", "doto", "..", "lazy-seq",
        ];
        for name in macros {
            self.add_symbol(name, SymbolKind::Macro, None);
            self.core_symbol_names.insert(name.to_string());
        }

        // Core functions (commonly used)
        let functions = [
            // Arithmetic
            "+", "-", "*", "/", "rem", "mod", "inc", "dec", "min", "max",
            // Comparison
            "=", "==", "not=", "<", ">", "<=", ">=",
            // Logic
            "not", "nil?", "some?", "true?", "false?",
            // Collections
            "count", "conj", "assoc", "dissoc", "get", "contains?", "empty?",
            "first", "rest", "next", "seq", "cons", "nth", "peek", "pop",
            "keys", "vals", "hash-map", "hash-set", "vector", "list",
            // Higher-order
            "map", "filter", "reduce", "remove", "take", "drop",
            "take-while", "drop-while", "iterate", "repeat", "repeatedly",
            "range", "concat", "mapcat", "apply", "partial", "comp",
            // Predicates
            "even?", "odd?", "pos?", "neg?", "zero?",
            "vector?", "map?", "set?", "seq?", "fn?",
            // Misc
            "identity", "constantly", "str", "pr-str", "print", "println",
            "type", "instance?", "hash",
        ];
        for name in functions {
            self.add_symbol(name, SymbolKind::Function, None);
            self.core_symbol_names.insert(name.to_string());
        }
    }

    /// Add a symbol to the table
    pub fn add_symbol(&mut self, name: &str, kind: SymbolKind, arity: Option<usize>) {
        self.symbols.insert(
            name.to_string(),
            SymbolEntry {
                name: name.to_string(),
                kind,
                namespace: None,
                arity,
            },
        );
    }

    /// Preload core.sus during startup
    ///
    /// Call this during REPL initialization to front-load parsing cost.
    pub fn preload_core(&mut self) -> Result<(), String> {
        self.compiler
            .preload_core()
            .map_err(|e| format!("Failed to load core.sus: {}", e))?;

        // Extract symbols from core.sus cache
        // Clone the data to avoid borrow checker issues
        let core_symbols: Vec<(String, SymbolKind, Option<usize>)> =
            if let Some(core_cache) = self.compiler.get_core_cache() {
                let mut symbols = Vec::new();

                for func in &core_cache.functions {
                    if !self.core_symbol_names.contains(&func.name) {
                        symbols.push((func.name.clone(), SymbolKind::Function, Some(func.params.len())));
                    }
                }
                for proto in &core_cache.protocols {
                    symbols.push((proto.name.clone(), SymbolKind::Protocol, None));
                    for method in &proto.methods {
                        symbols.push((method.name.clone(), SymbolKind::Function, None));
                    }
                }
                for dt in &core_cache.deftypes {
                    symbols.push((dt.name.clone(), SymbolKind::Type, None));
                    let ctor_name = format!("->{}", dt.name);
                    symbols.push((ctor_name, SymbolKind::Function, Some(dt.fields.len())));
                }

                symbols
            } else {
                Vec::new()
            };

        // Now add the symbols (no borrow conflict)
        for (name, kind, arity) in core_symbols {
            self.add_symbol(&name, kind, arity);
            self.core_symbol_names.insert(name);
        }

        Ok(())
    }

    /// Build complete source for compilation
    pub fn build_source(&self, expr: &str) -> String {
        let mut source = String::new();

        // 0. Inject *ns* binding for current namespace
        source.push_str(&format!("(def *ns* '{})\n", self.current_ns));

        // 1. Add all loaded namespace files
        for (_ns, loaded) in &self.loaded_namespaces {
            source.push_str(&loaded.source);
            source.push('\n');
        }

        // 2. Add accumulated REPL definitions (by name, not concatenated)
        for (_ns, defs) in &self.ns_definitions {
            for (_name, def_source) in defs {
                source.push_str(def_source);
                source.push('\n');
            }
        }

        // 3. Add current expression
        source.push_str(expr);
        source
    }

    /// Accumulate a definition into current namespace
    ///
    /// Returns Some(name) if a definition was redefined, None otherwise.
    pub fn accumulate_definition(&mut self, def: &str) -> Option<String> {
        if let Some(name) = extract_def_name(def) {
            let ns_defs = self.ns_definitions
                .entry(self.current_ns.clone())
                .or_default();

            let was_redefined = ns_defs.contains_key(&name);
            ns_defs.insert(name.clone(), def.to_string());

            // Extract symbol kind and add to table
            let kind = if def.starts_with("(defn ") || def.starts_with("(defmacro ") {
                if def.starts_with("(defmacro ") {
                    SymbolKind::Macro
                } else {
                    SymbolKind::Function
                }
            } else if def.starts_with("(deftype ") {
                SymbolKind::Type
            } else if def.starts_with("(defprotocol ") {
                SymbolKind::Protocol
            } else {
                SymbolKind::Var
            };
            self.add_symbol(&name, kind, None);

            if was_redefined {
                return Some(name);
            }
        }
        None
    }

    /// Compile an expression with caching
    ///
    /// Retain the complete artifact, including shared-runtime dependency modules.
    /// Returns (compiled artifact, was_cache_hit). This source-replay fixture is
    /// not the shipped persistent REPL.
    pub fn compile_cached(&mut self, expr: &str) -> Result<(Arc<CompiledExpr>, bool), String> {
        let full_source = self.build_source(expr);
        let hash = hash_source(&full_source);

        // Check cache
        if let Some(entry) = self.wasm_cache.get(&hash) {
            self.cache_hits += 1;
            // Move to end of LRU
            if let Some(pos) = self.cache_order.iter().position(|&h| h == hash) {
                self.cache_order.remove(pos);
            }
            self.cache_order.push(hash);
            return Ok((Arc::clone(&entry.compiled), true));
        }

        // Cache miss - compile
        self.cache_misses += 1;
        let compiled = Arc::new(self.compiler
            // This module is retained only as prototype regression fixtures.
            // The shipped REPL uses portable_session, never this source replay.
            .compile_expr_with_info(&full_source)
            .map_err(|e| format!("{}", e))?);

        // Cache result
        self.cache_insert(hash, CacheEntry { compiled: Arc::clone(&compiled) });

        Ok((compiled, false))
    }

    /// Insert into cache with LRU eviction
    fn cache_insert(&mut self, hash: u64, entry: CacheEntry) {
        // Evict if at capacity
        while self.wasm_cache.len() >= MAX_CACHE_ENTRIES {
            if let Some(oldest) = self.cache_order.first().copied() {
                self.cache_order.remove(0);
                self.wasm_cache.remove(&oldest);
            } else {
                break;
            }
        }

        self.wasm_cache.insert(hash, entry);
        self.cache_order.push(hash);
    }

    /// Clear the WASM cache (e.g., after namespace changes)
    pub fn clear_cache(&mut self) {
        self.wasm_cache.clear();
        self.cache_order.clear();
    }

    /// Check loaded namespace files for modifications and reload any that changed.
    ///
    /// Returns the list of namespace names that were reloaded.
    pub fn check_for_reloads(&mut self) -> Vec<String> {
        let stale: Vec<(String, PathBuf)> = self.loaded_namespaces.iter()
            .filter_map(|(ns, loaded)| {
                match std::fs::metadata(&loaded.path).and_then(|m| m.modified()) {
                    Ok(mtime) if mtime > loaded.mtime => Some((ns.clone(), loaded.path.clone())),
                    _ => None,
                }
            })
            .collect();

        for (ns, path) in &stale {
            if let Ok(source) = std::fs::read_to_string(path) {
                let mtime = std::fs::metadata(path)
                    .and_then(|m| m.modified())
                    .unwrap_or_else(|_| SystemTime::now());
                self.loaded_namespaces.insert(ns.clone(), LoadedNamespace {
                    path: path.clone(), mtime, source,
                });
            }
        }

        let ns_names: Vec<String> = stale.into_iter().map(|(ns, _)| ns).collect();
        if !ns_names.is_empty() {
            self.clear_cache();
        }
        ns_names
    }

    /// Get cache statistics as a formatted string
    pub fn cache_stats(&self) -> String {
        let total = self.cache_hits + self.cache_misses;
        let hit_rate = if total > 0 {
            100.0 * self.cache_hits as f64 / total as f64
        } else {
            0.0
        };
        format!(
            "Cache: {} hits, {} misses ({:.1}% hit rate), {} entries",
            self.cache_hits,
            self.cache_misses,
            hit_rate,
            self.wasm_cache.len()
        )
    }

    /// Get all symbols matching a prefix (for tab completion)
    pub fn complete_symbol(&self, prefix: &str) -> Vec<&SymbolEntry> {
        let mut matches: Vec<_> = self.symbols
            .values()
            .filter(|s| s.name.starts_with(prefix))
            .collect();
        matches.sort_by(|a, b| a.name.cmp(&b.name));
        matches
    }
}

impl Default for SessionState {
    fn default() -> Self {
        Self::new()
    }
}

/// Hash source code for cache key
fn hash_source(source: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    hasher.finish()
}

/// Extract definition name from source
fn extract_def_name(def: &str) -> Option<String> {
    // Simple extraction: find the name after (def, (defn, etc.
    let prefixes = [
        "(def ", "(defn ", "(deftype ", "(defprotocol ", "(defmacro ", "(extend-type ",
    ];

    for prefix in prefixes {
        if def.starts_with(prefix) {
            let rest = &def[prefix.len()..];
            // Skip metadata like ^:export
            let rest = if rest.starts_with('^') {
                // Find the name after metadata
                rest.split_whitespace().nth(1)?
            } else {
                rest.split_whitespace().next()?
            };
            // Clean up any trailing brackets
            let name = rest.trim_end_matches(|c: char| c == '[' || c == '(');
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_def_name() {
        assert_eq!(extract_def_name("(defn foo [] 42)"), Some("foo".to_string()));
        assert_eq!(extract_def_name("(def x 10)"), Some("x".to_string()));
        assert_eq!(extract_def_name("(deftype Point [x y])"), Some("Point".to_string()));
        assert_eq!(extract_def_name("(defn ^:export add [a b] (+ a b))"), Some("add".to_string()));
    }

    #[test]
    fn test_hash_source() {
        let source1 = "(+ 1 2)";
        let source2 = "(+ 1 2)";
        let source3 = "(+ 1 3)";

        assert_eq!(hash_source(source1), hash_source(source2));
        assert_ne!(hash_source(source1), hash_source(source3));
    }

    #[test]
    fn test_compilation_cache_retains_complete_artifact() {
        let mut state = SessionState::new();
        let source = "(defmacro answer [] 42) (answer)";
        let (first, hit) = state.compile_cached(source).unwrap();
        assert!(!hit);
        let (cached, hit) = state.compile_cached(source).unwrap();
        assert!(hit);
        assert!(Arc::ptr_eq(&first, &cached));
        assert!(first.prepared.as_ref().unwrap().modules().count() > 1);
        state.clear_cache();
        drop(cached);
        let artifact = Arc::try_unwrap(first).unwrap();
        let mut session = suss_compile::portable_session::Session::new().unwrap();
        let value = artifact.execute(&mut session).unwrap().unwrap();
        session.collect().unwrap();
        let observed = session.inspect(&value, |mut store, value| {
            let number = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            Ok(number.field(&mut store, 0)?.unwrap_f64())
        }).unwrap();
        assert_eq!(observed, 42.0);
    }

    #[test]
    fn test_session_state_new() {
        let state = SessionState::new();
        assert_eq!(state.current_ns, "user");
        assert!(!state.symbols.is_empty());
        assert!(state.symbols.contains_key("+"));
        assert!(state.symbols.contains_key("defn"));
    }

    #[test]
    fn test_accumulate_definition() {
        let mut state = SessionState::new();
        let redefined = state.accumulate_definition("(defn foo [] 42)");

        assert!(redefined.is_none()); // First definition, not a redefinition
        assert!(state.ns_definitions.contains_key("user"));
        assert!(state.ns_definitions["user"].contains_key("foo"));
        assert_eq!(state.ns_definitions["user"]["foo"], "(defn foo [] 42)");
        assert!(state.symbols.contains_key("foo"));
    }

    #[test]
    fn test_redefinition_detected() {
        let mut state = SessionState::new();
        let redefined1 = state.accumulate_definition("(defn add [a b] (+ a b))");
        assert!(redefined1.is_none());

        let redefined2 = state.accumulate_definition("(defn add [a b] (- a b))");
        assert_eq!(redefined2, Some("add".to_string()));

        // Only one entry should exist
        assert_eq!(state.ns_definitions["user"].len(), 1);
        assert_eq!(state.ns_definitions["user"]["add"], "(defn add [a b] (- a b))");
    }

    #[test]
    fn test_ns_var_injected() {
        let state = SessionState::new();
        let source = state.build_source("(+ 1 2)");

        // *ns* should be injected at the start
        assert!(source.starts_with("(def *ns* 'user)\n"));
        assert!(source.contains("(+ 1 2)"));
    }

    #[test]
    fn test_complete_symbol() {
        let state = SessionState::new();

        let matches = state.complete_symbol("con");
        let names: Vec<_> = matches.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"conj"));
        assert!(names.contains(&"cons"));
        assert!(names.contains(&"contains?"));
    }
}
