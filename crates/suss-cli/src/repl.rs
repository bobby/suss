//! REPL state management for stateful evaluation
//!
//! Maintains state across REPL expressions including:
//! - Accumulated definitions (defn, def, deftype, etc.)
//! - Current namespace context
//! - Loaded namespace files
//! - Namespace aliases from requires

use std::collections::HashMap;
use std::path::PathBuf;

/// Persistent REPL state that accumulates across expressions
pub struct ReplState {
    /// Accumulated definitions per namespace: namespace -> (def-name -> source)
    pub ns_definitions: HashMap<String, HashMap<String, String>>,

    /// Current namespace context
    pub current_ns: String,

    /// Namespace aliases from requires: alias -> full namespace name
    pub ns_aliases: HashMap<String, String>,

    /// Loaded namespace files: namespace -> source content
    pub loaded_namespaces: HashMap<String, String>,

    /// Source paths for namespace resolution
    pub src_paths: Vec<PathBuf>,
}

impl ReplState {
    pub fn new() -> Self {
        Self {
            ns_definitions: HashMap::new(),
            current_ns: "user".to_string(),
            ns_aliases: HashMap::new(),
            loaded_namespaces: HashMap::new(),
            src_paths: vec![PathBuf::from("src")],
        }
    }

    /// Build complete source for compilation
    ///
    /// Combines all loaded namespaces, accumulated definitions, and the current expression
    pub fn build_source(&self, expr: &str) -> String {
        let mut source = String::new();

        // 0. Inject *ns* binding for current namespace
        source.push_str(&format!("(def *ns* '{})\n", self.current_ns));

        // 1. Add all loaded namespace files
        for (_ns, ns_source) in &self.loaded_namespaces {
            source.push_str(ns_source);
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

            if was_redefined {
                return Some(name);
            }
        }
        None
    }
}

/// Extract definition name from source
fn extract_def_name(def: &str) -> Option<String> {
    let prefixes = [
        "(def ", "(defn ", "(deftype ", "(defprotocol ", "(defmacro ", "(extend-type ",
    ];

    for prefix in prefixes {
        if def.starts_with(prefix) {
            let rest = &def[prefix.len()..];
            // Skip metadata like ^:export
            let rest = if rest.starts_with('^') {
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

impl Default for ReplState {
    fn default() -> Self {
        Self::new()
    }
}

/// Handle (in-ns 'namespace-name)
pub fn handle_in_ns(state: &mut ReplState, input: &str) -> Result<String, String> {
    let ns_name = parse_quoted_symbol(input, "in-ns")?;

    // Switch context
    state.current_ns = ns_name.clone();

    // Initialize namespace if needed
    state.ns_definitions.entry(ns_name.clone()).or_default();

    Ok(ns_name)
}

/// Handle (require '[namespace :as alias])
pub fn handle_require(state: &mut ReplState, input: &str) -> Result<String, String> {
    let spec = parse_require_spec(input)?;

    // Check if this is WASI (contains ':')
    if spec.namespace.contains(':') {
        // WASI requires are handled by compiler, just record alias
        if let Some(alias) = spec.alias {
            state.ns_aliases.insert(alias, spec.namespace.clone());
        }
        return Ok(format!("WASI import: {}", spec.namespace));
    }

    // Already loaded?
    if state.loaded_namespaces.contains_key(&spec.namespace) {
        if let Some(alias) = spec.alias {
            state.ns_aliases.insert(alias, spec.namespace.clone());
        }
        return Ok(format!("Already loaded: {}", spec.namespace));
    }

    // Find file using ns_to_path
    let path = suss_compile::Compiler::ns_to_path(&spec.namespace, &state.src_paths)
        .ok_or_else(|| format!("Namespace '{}' not found in {:?}",
            spec.namespace, state.src_paths))?;

    // Read file
    let source = std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

    // Store loaded namespace
    state.loaded_namespaces.insert(spec.namespace.clone(), source);

    // Record alias
    if let Some(alias) = spec.alias {
        state.ns_aliases.insert(alias, spec.namespace.clone());
    }

    Ok(format!("Loaded: {}", spec.namespace))
}

/// Check if input is a definition form that should be accumulated
pub fn is_definition(input: &str) -> bool {
    input.starts_with("(def ") ||
    input.starts_with("(defn ") ||
    input.starts_with("(deftype ") ||
    input.starts_with("(defprotocol ") ||
    input.starts_with("(defmacro ") ||
    input.starts_with("(extend-type ")
}

/// Parsed require specification
struct RequireSpec {
    namespace: String,
    alias: Option<String>,
    #[allow(dead_code)]
    refers: Vec<String>,
}

/// Parse a quoted symbol from input like (in-ns 'name)
fn parse_quoted_symbol(input: &str, form_name: &str) -> Result<String, String> {
    // Find 'symbol in input
    let quote_pos = input.find('\'')
        .ok_or_else(|| format!("({} ...) requires a quoted symbol", form_name))?;
    let rest = &input[quote_pos + 1..];

    // Find end of symbol (whitespace or close paren)
    let end = rest.find(|c: char| c.is_whitespace() || c == ')')
        .unwrap_or(rest.len());
    let name = rest[..end].trim();

    if name.is_empty() {
        return Err(format!("({} ...) requires a namespace name", form_name));
    }

    Ok(name.to_string())
}

/// Parse (require '[namespace :as alias :refer [sym1 sym2]])
fn parse_require_spec(input: &str) -> Result<RequireSpec, String> {
    // Find the vector content
    let start = input.find('[').ok_or("require needs a vector spec")?;
    let end = input.rfind(']').ok_or("require needs a closing ]")?;
    let content = &input[start + 1..end];

    let parts: Vec<&str> = content.split_whitespace().collect();
    if parts.is_empty() {
        return Err("Empty require spec".into());
    }

    let namespace = parts[0].to_string();
    let mut alias = None;
    let mut refers = Vec::new();

    let mut i = 1;
    while i < parts.len() {
        match parts[i] {
            ":as" if i + 1 < parts.len() => {
                alias = Some(parts[i + 1].to_string());
                i += 2;
            }
            ":refer" if i + 1 < parts.len() => {
                // Parse [sym1 sym2] or :all
                if parts[i + 1] == ":all" {
                    // :refer :all - handled specially (could mark all symbols as referred)
                    i += 2;
                } else {
                    // Find bracketed list
                    i += 1;
                    while i < parts.len() && !parts[i].starts_with(':') {
                        let sym = parts[i].trim_matches(|c| c == '[' || c == ']');
                        if !sym.is_empty() {
                            refers.push(sym.to_string());
                        }
                        i += 1;
                    }
                }
            }
            _ => i += 1,
        }
    }

    Ok(RequireSpec { namespace, alias, refers })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_quoted_symbol() {
        assert_eq!(
            parse_quoted_symbol("(in-ns 'myapp.core)", "in-ns").unwrap(),
            "myapp.core"
        );
        assert_eq!(
            parse_quoted_symbol("(in-ns 'user)", "in-ns").unwrap(),
            "user"
        );
    }

    #[test]
    fn test_parse_require_spec_simple() {
        let spec = parse_require_spec("(require '[myapp.utils])").unwrap();
        assert_eq!(spec.namespace, "myapp.utils");
        assert!(spec.alias.is_none());
    }

    #[test]
    fn test_parse_require_spec_with_alias() {
        let spec = parse_require_spec("(require '[myapp.utils :as u])").unwrap();
        assert_eq!(spec.namespace, "myapp.utils");
        assert_eq!(spec.alias, Some("u".to_string()));
    }

    #[test]
    fn test_is_definition() {
        assert!(is_definition("(defn foo [] 42)"));
        assert!(is_definition("(def x 10)"));
        assert!(is_definition("(deftype Point [x y])"));
        assert!(is_definition("(defprotocol IFoo (-foo [this]))"));
        assert!(is_definition("(defmacro when [test & body] `(if ~test (do ~@body)))"));
        assert!(is_definition("(extend-type String IFoo (-foo [this] this))"));
        assert!(!is_definition("(+ 1 2)"));
        assert!(!is_definition("(foo)"));
    }

    #[test]
    fn test_repl_state_build_source() {
        let mut state = ReplState::new();
        state.accumulate_definition("(defn foo [] 42)");
        state.accumulate_definition("(def x 10)");

        let source = state.build_source("(foo)");
        assert!(source.contains("(defn foo [] 42)"));
        assert!(source.contains("(def x 10)"));
        assert!(source.contains("(foo)"));
    }

    #[test]
    fn test_in_ns_switches_namespace() {
        let mut state = ReplState::new();
        assert_eq!(state.current_ns, "user");

        handle_in_ns(&mut state, "(in-ns 'myapp.core)").unwrap();
        assert_eq!(state.current_ns, "myapp.core");
    }
}
