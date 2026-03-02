//! Macro expansion for the Suss compiler
//!
//! This module handles:
//! - Syntax-quote expansion (` forms with ~ and ~@)
//! - Macro definitions (defmacro)
//! - Macro expansion phase in the compilation pipeline
//!
//! Following ClojureScript's Clojure-hosted model, macros run at compile time
//! using a tree-walking evaluator.

use crate::error::{CompileError, CompileResult};
use crate::eval::MacroEvaluator;
use suss_core::{Edn, Env, Keyword, Symbol};
use std::collections::HashMap;

/// A macro definition
#[derive(Debug, Clone)]
pub struct MacroDef {
    /// The macro name
    pub name: String,
    /// Parameter names (before &)
    pub params: Vec<String>,
    /// Rest parameter name (after &), if any
    pub rest_param: Option<String>,
    /// The macro body (unevaluated)
    pub body: Edn,
}

/// A binding group in a for comprehension
#[derive(Debug, Clone)]
struct ForBindingGroup {
    /// The binding pattern (symbol or destructuring pattern)
    pattern: Edn,
    /// The collection to iterate over
    collection: Edn,
    /// :when predicates
    when_clauses: Vec<Edn>,
    /// :let bindings (flattened [sym expr ...])
    let_bindings: Vec<Edn>,
}

/// Macro expansion environment
pub struct MacroEnv {
    /// Registered macros
    macros: HashMap<String, MacroDef>,
    /// Counter for generating unique symbols
    gensym_counter: u64,
    /// Current namespace for symbol resolution
    current_ns: Option<String>,
    /// The evaluator for running macro bodies
    evaluator: MacroEvaluator,
}

impl MacroEnv {
    /// Create a new macro environment
    pub fn new() -> Self {
        let mut env = MacroEnv {
            macros: HashMap::new(),
            gensym_counter: 0,
            current_ns: None,
            evaluator: MacroEvaluator::new(),
        };
        env.register_core_macros();
        env
    }

    /// Register the core macros (when, when-not, cond, and, or)
    fn register_core_macros(&mut self) {
        // These are manually constructed rather than parsed to avoid bootstrap issues
        // with syntax-quote during macro system initialization.

        // (defmacro when [test & body]
        //   (list 'if test (cons 'do body) nil))
        self.define_macro(MacroDef {
            name: "when".to_string(),
            params: vec!["test".to_string()],
            rest_param: Some("body".to_string()),
            body: Edn::List(vec![
                Edn::Symbol(Symbol::new("list")),
                Edn::List(vec![
                    Edn::Symbol(Symbol::new("quote")),
                    Edn::Symbol(Symbol::new("if")),
                ]),
                Edn::Symbol(Symbol::new("test")),
                Edn::List(vec![
                    Edn::Symbol(Symbol::new("cons")),
                    Edn::List(vec![
                        Edn::Symbol(Symbol::new("quote")),
                        Edn::Symbol(Symbol::new("do")),
                    ]),
                    Edn::Symbol(Symbol::new("body")),
                ]),
                Edn::Nil,
            ]),
        });

        // (defmacro when-not [test & body]
        //   (list 'if test nil (cons 'do body)))
        self.define_macro(MacroDef {
            name: "when-not".to_string(),
            params: vec!["test".to_string()],
            rest_param: Some("body".to_string()),
            body: Edn::List(vec![
                Edn::Symbol(Symbol::new("list")),
                Edn::List(vec![
                    Edn::Symbol(Symbol::new("quote")),
                    Edn::Symbol(Symbol::new("if")),
                ]),
                Edn::Symbol(Symbol::new("test")),
                Edn::Nil,
                Edn::List(vec![
                    Edn::Symbol(Symbol::new("cons")),
                    Edn::List(vec![
                        Edn::Symbol(Symbol::new("quote")),
                        Edn::Symbol(Symbol::new("do")),
                    ]),
                    Edn::Symbol(Symbol::new("body")),
                ]),
            ]),
        });

        // For and/or/cond, we need more complex recursive expansion
        // These will use the special _and_impl, _or_impl, _cond_impl helpers

        // (defmacro and [& args]
        //   (cond
        //     (empty? args) true
        //     (= (count args) 1) (first args)
        //     :else (list 'if (first args) (cons 'and (rest args)) (first args))))
        // Note: This is a simplified version that doesn't short-circuit properly for falsy values
        // We'll use a simpler approach with let bindings
        self.define_macro(MacroDef {
            name: "and".to_string(),
            params: vec![],
            rest_param: Some("args".to_string()),
            body: Edn::List(vec![
                Edn::Symbol(Symbol::new("_and_impl")),
                Edn::Symbol(Symbol::new("args")),
            ]),
        });

        // (defmacro or [& args]
        //   (_or_impl args))
        self.define_macro(MacroDef {
            name: "or".to_string(),
            params: vec![],
            rest_param: Some("args".to_string()),
            body: Edn::List(vec![
                Edn::Symbol(Symbol::new("_or_impl")),
                Edn::Symbol(Symbol::new("args")),
            ]),
        });

        // (defmacro cond [& clauses]
        //   (_cond_impl clauses))
        self.define_macro(MacroDef {
            name: "cond".to_string(),
            params: vec![],
            rest_param: Some("clauses".to_string()),
            body: Edn::List(vec![
                Edn::Symbol(Symbol::new("_cond_impl")),
                Edn::Symbol(Symbol::new("clauses")),
            ]),
        });

        // (defmacro case [e & clauses]
        //   (_case_impl e clauses))
        self.define_macro(MacroDef {
            name: "case".to_string(),
            params: vec!["e".to_string()],
            rest_param: Some("clauses".to_string()),
            body: Edn::List(vec![
                Edn::Symbol(Symbol::new("_case_impl")),
                Edn::Symbol(Symbol::new("e")),
                Edn::Symbol(Symbol::new("clauses")),
            ]),
        });

        // Threading macros
        // (defmacro -> [x & forms]
        //   (_thread_first_impl x forms))
        self.define_macro(MacroDef {
            name: "->".to_string(),
            params: vec!["x".to_string()],
            rest_param: Some("forms".to_string()),
            body: Edn::List(vec![
                Edn::Symbol(Symbol::new("_thread_first_impl")),
                Edn::Symbol(Symbol::new("x")),
                Edn::Symbol(Symbol::new("forms")),
            ]),
        });

        // (defmacro ->> [x & forms]
        //   (_thread_last_impl x forms))
        self.define_macro(MacroDef {
            name: "->>".to_string(),
            params: vec!["x".to_string()],
            rest_param: Some("forms".to_string()),
            body: Edn::List(vec![
                Edn::Symbol(Symbol::new("_thread_last_impl")),
                Edn::Symbol(Symbol::new("x")),
                Edn::Symbol(Symbol::new("forms")),
            ]),
        });

        // Conditional binding macros
        // (defmacro when-let [[sym expr] & body]
        //   (_when_let_impl sym expr body))
        self.define_macro(MacroDef {
            name: "when-let".to_string(),
            params: vec!["binding".to_string()],
            rest_param: Some("body".to_string()),
            body: Edn::List(vec![
                Edn::Symbol(Symbol::new("_when_let_impl")),
                Edn::Symbol(Symbol::new("binding")),
                Edn::Symbol(Symbol::new("body")),
            ]),
        });

        // (defmacro if-let [[sym expr] then else]
        //   (_if_let_impl binding then else))
        self.define_macro(MacroDef {
            name: "if-let".to_string(),
            params: vec!["binding".to_string(), "then".to_string()],
            rest_param: Some("else_clause".to_string()),
            body: Edn::List(vec![
                Edn::Symbol(Symbol::new("_if_let_impl")),
                Edn::Symbol(Symbol::new("binding")),
                Edn::Symbol(Symbol::new("then")),
                Edn::Symbol(Symbol::new("else_clause")),
            ]),
        });

        // Conditional threading macros

        // (defmacro cond-> [expr & clauses]
        //   (_cond_thread_first_impl expr clauses))
        self.define_macro(MacroDef {
            name: "cond->".to_string(),
            params: vec!["expr".to_string()],
            rest_param: Some("clauses".to_string()),
            body: Edn::List(vec![
                Edn::Symbol(Symbol::new("_cond_thread_first_impl")),
                Edn::Symbol(Symbol::new("expr")),
                Edn::Symbol(Symbol::new("clauses")),
            ]),
        });

        // (defmacro cond->> [expr & clauses]
        //   (_cond_thread_last_impl expr clauses))
        self.define_macro(MacroDef {
            name: "cond->>".to_string(),
            params: vec!["expr".to_string()],
            rest_param: Some("clauses".to_string()),
            body: Edn::List(vec![
                Edn::Symbol(Symbol::new("_cond_thread_last_impl")),
                Edn::Symbol(Symbol::new("expr")),
                Edn::Symbol(Symbol::new("clauses")),
            ]),
        });

        // (defmacro some-> [expr & forms]
        //   (_some_thread_first_impl expr forms))
        self.define_macro(MacroDef {
            name: "some->".to_string(),
            params: vec!["expr".to_string()],
            rest_param: Some("forms".to_string()),
            body: Edn::List(vec![
                Edn::Symbol(Symbol::new("_some_thread_first_impl")),
                Edn::Symbol(Symbol::new("expr")),
                Edn::Symbol(Symbol::new("forms")),
            ]),
        });

        // (defmacro some->> [expr & forms]
        //   (_some_thread_last_impl expr forms))
        self.define_macro(MacroDef {
            name: "some->>".to_string(),
            params: vec!["expr".to_string()],
            rest_param: Some("forms".to_string()),
            body: Edn::List(vec![
                Edn::Symbol(Symbol::new("_some_thread_last_impl")),
                Edn::Symbol(Symbol::new("expr")),
                Edn::Symbol(Symbol::new("forms")),
            ]),
        });

        // (defmacro as-> [expr name & forms]
        //   (_as_thread_impl expr name forms))
        self.define_macro(MacroDef {
            name: "as->".to_string(),
            params: vec!["expr".to_string(), "name".to_string()],
            rest_param: Some("forms".to_string()),
            body: Edn::List(vec![
                Edn::Symbol(Symbol::new("_as_thread_impl")),
                Edn::Symbol(Symbol::new("expr")),
                Edn::Symbol(Symbol::new("name")),
                Edn::Symbol(Symbol::new("forms")),
            ]),
        });

        // (defmacro doseq [seq-exprs & body]
        //   (_doseq_impl seq-exprs body))
        self.define_macro(MacroDef {
            name: "doseq".to_string(),
            params: vec!["seq_exprs".to_string()],
            rest_param: Some("body".to_string()),
            body: Edn::List(vec![
                Edn::Symbol(Symbol::new("_doseq_impl")),
                Edn::Symbol(Symbol::new("seq_exprs")),
                Edn::Symbol(Symbol::new("body")),
            ]),
        });

        // (defmacro when-first [[sym coll] & body]
        //   (_when_first_impl binding body))
        self.define_macro(MacroDef {
            name: "when-first".to_string(),
            params: vec!["binding".to_string()],
            rest_param: Some("body".to_string()),
            body: Edn::List(vec![
                Edn::Symbol(Symbol::new("_when_first_impl")),
                Edn::Symbol(Symbol::new("binding")),
                Edn::Symbol(Symbol::new("body")),
            ]),
        });
    }

    /// Set the current namespace
    pub fn set_namespace(&mut self, ns: &str) {
        self.current_ns = Some(ns.to_string());
    }

    /// Generate a unique symbol (for foo# patterns)
    pub fn gensym(&mut self, base: &str) -> Symbol {
        let id = self.gensym_counter;
        self.gensym_counter += 1;
        Symbol::new(format!("{}__{}__auto__", base, id))
    }

    /// Register a macro definition
    pub fn define_macro(&mut self, def: MacroDef) {
        self.macros.insert(def.name.clone(), def);
    }

    /// Check if a symbol names a macro
    pub fn is_macro(&self, name: &str) -> bool {
        self.macros.contains_key(name)
    }

    /// Get a macro definition
    pub fn get_macro(&self, name: &str) -> Option<&MacroDef> {
        self.macros.get(name)
    }

    /// Expand all expressions, processing defmacro and expanding macro calls
    pub fn expand_all(&mut self, exprs: Vec<Edn>) -> CompileResult<Vec<Edn>> {
        let mut result = Vec::new();
        for expr in exprs {
            if let Some(expanded) = self.expand_toplevel(expr)? {
                result.push(expanded);
            }
        }
        Ok(result)
    }

    /// Expand a top-level expression
    /// Returns None for defmacro (consumed but not emitted)
    fn expand_toplevel(&mut self, expr: Edn) -> CompileResult<Option<Edn>> {
        // Check for defmacro
        if let Edn::List(ref items) = expr {
            if let Some(Edn::Symbol(sym)) = items.first() {
                if sym.name == "defmacro" {
                    self.process_defmacro(items)?;
                    return Ok(None);
                }
                if sym.name == "defn" {
                    // Expand defn to (def name (fn ...))
                    let expanded = self.expand_defn(items)?;
                    return Ok(Some(self.expand(expanded)?));
                }
            }
        }

        // Otherwise expand normally
        Ok(Some(self.expand(expr)?))
    }

    /// Expand defn to (def name (fn params body))
    /// Single-arity: (defn name [params] body...)
    /// Multi-arity:  (defn name ([params1] body1) ([params2] body2)...)
    /// With metadata: (defn ^:export name [params] body...)
    /// With docstring: (defn name "docstring" [params] body...)
    fn expand_defn(&self, items: &[Edn]) -> CompileResult<Edn> {
        // defn requires at least: defn name [params] body  or  defn name ([params] body)
        if items.len() < 3 {
            return Err(CompileError::MacroExpansion(
                "defn requires name and body".into(),
            ));
        }

        let mut idx = 1;
        let mut metadata: Vec<Edn> = Vec::new();

        // Collect metadata symbols (^:export, ^i32, etc.)
        while idx < items.len() {
            if let Edn::Symbol(s) = &items[idx] {
                if s.name.starts_with('^') {
                    metadata.push(items[idx].clone());
                    idx += 1;
                    continue;
                }
            }
            break;
        }

        // Name must be a symbol
        let name = match &items[idx] {
            Edn::Symbol(_) => items[idx].clone(),
            _ => {
                return Err(CompileError::MacroExpansion(
                    "defn name must be a symbol".into(),
                ))
            }
        };
        idx += 1;

        // Capture optional docstring
        let docstring = if idx < items.len() {
            if let Edn::String(doc) = &items[idx] {
                idx += 1;
                Some(Edn::String(doc.clone()))
            } else {
                None
            }
        } else {
            None
        };

        if idx >= items.len() {
            return Err(CompileError::MacroExpansion(
                "defn requires parameters and body".into(),
            ));
        }

        // Check for multi-arity form: (defn name ([params] body) ([params] body) ...)
        // Detection: next item is a List where first element is a Vector
        let is_multi_arity = if let Edn::List(clause) = &items[idx] {
            matches!(clause.first(), Some(Edn::Vector(_)))
        } else {
            false
        };

        if is_multi_arity {
            // Multi-arity: collect all arity clauses and build (fn ([p1] b1) ([p2] b2) ...)
            let clauses: Vec<Edn> = items[idx..].to_vec();
            let mut fn_form = vec![Edn::Symbol(Symbol::new("fn"))];
            fn_form.extend(clauses);

            // Build (def [metadata...] name [docstring] (fn ...)) form
            let mut def_form = vec![Edn::Symbol(Symbol::new("def"))];
            def_form.extend(metadata);
            def_form.push(name);
            if let Some(doc) = docstring {
                def_form.push(doc);
            }
            def_form.push(Edn::List(fn_form));

            return Ok(Edn::List(def_form));
        }

        // Single-arity: params must be a vector
        let params = match &items[idx] {
            Edn::Vector(_) => items[idx].clone(),
            _ => {
                return Err(CompileError::MacroExpansion(
                    "defn params must be a vector".into(),
                ))
            }
        };
        idx += 1;

        // Body is everything after params
        let body: Vec<Edn> = items[idx..].to_vec();
        if body.is_empty() {
            return Err(CompileError::MacroExpansion("defn requires a body".into()));
        }

        // Build (fn params body...) form
        let mut fn_form = vec![Edn::Symbol(Symbol::new("fn")), params];
        fn_form.extend(body);

        // Build (def [metadata...] name [docstring] (fn ...)) form
        // Docstring comes after name, before fn body (matching defn syntax)
        let mut def_form = vec![Edn::Symbol(Symbol::new("def"))];
        def_form.extend(metadata);
        def_form.push(name);
        if let Some(doc) = docstring {
            def_form.push(doc);
        }
        def_form.push(Edn::List(fn_form));

        Ok(Edn::List(def_form))
    }

    /// Process a defmacro form
    fn process_defmacro(&mut self, items: &[Edn]) -> CompileResult<()> {
        // (defmacro name [params] body)
        // or (defmacro name docstring [params] body)
        if items.len() < 4 {
            return Err(CompileError::MacroExpansion(
                "defmacro requires name, params, and body".into(),
            ));
        }

        let name = match &items[1] {
            Edn::Symbol(s) => s.name.clone(),
            _ => {
                return Err(CompileError::MacroExpansion(
                    "defmacro name must be a symbol".into(),
                ))
            }
        };

        // Skip optional docstring
        let (params_idx, body_idx) = if matches!(&items[2], Edn::String(_)) {
            (3, 4)
        } else {
            (2, 3)
        };

        // Parse parameters
        let (params, rest_param) = match &items[params_idx] {
            Edn::Vector(v) => self.parse_macro_params(v)?,
            _ => {
                return Err(CompileError::MacroExpansion(
                    "defmacro params must be a vector".into(),
                ))
            }
        };

        // Body is everything after params wrapped in (do ...)
        let body = if items.len() == body_idx + 1 {
            items[body_idx].clone()
        } else {
            let body_exprs: Vec<Edn> = items[body_idx..].to_vec();
            let mut do_form = vec![Edn::Symbol(Symbol::new("do"))];
            do_form.extend(body_exprs);
            Edn::List(do_form)
        };

        self.define_macro(MacroDef {
            name,
            params,
            rest_param,
            body,
        });

        Ok(())
    }

    /// Parse macro parameter list
    fn parse_macro_params(&self, params: &[Edn]) -> CompileResult<(Vec<String>, Option<String>)> {
        let mut regular = Vec::new();
        let mut rest = None;
        let mut saw_ampersand = false;

        for param in params {
            match param {
                Edn::Symbol(s) => {
                    let name = s.name.as_str();
                    if name == "&" {
                        saw_ampersand = true;
                    } else if saw_ampersand {
                        if rest.is_some() {
                            return Err(CompileError::MacroExpansion(
                                "Multiple rest parameters in macro".into(),
                            ));
                        }
                        rest = Some(name.to_string());
                    } else {
                        regular.push(name.to_string());
                    }
                }
                _ => {
                    return Err(CompileError::MacroExpansion(
                        "Macro parameters must be symbols".into(),
                    ))
                }
            }
        }

        Ok((regular, rest))
    }

    /// Check if a binding pattern requires destructuring
    fn needs_destructuring(pattern: &Edn) -> bool {
        match pattern {
            Edn::Vector(_) => true,
            Edn::Map(_) => true,
            _ => false,
        }
    }

    /// Generate destructured bindings for a vector pattern
    /// Returns a list of (symbol, value-expr) pairs
    fn destructure_vector(&mut self, pattern: &[Edn], value_sym: &Symbol) -> CompileResult<Vec<(Edn, Edn)>> {
        let mut bindings = Vec::new();
        let mut idx = 0;
        let mut saw_rest = false;

        for elem in pattern {
            if let Edn::Symbol(sym) = elem {
                if sym.name == "&" {
                    saw_rest = true;
                    continue;
                }
                if saw_rest {
                    // This is the rest binding: bind to (drop idx value)
                    bindings.push((
                        Edn::Symbol(sym.clone()),
                        Edn::List(vec![
                            Edn::Symbol(Symbol::new("drop")),
                            Edn::Number(suss_core::Number::from_i64(idx as i64)),
                            Edn::Symbol(value_sym.clone()),
                        ]),
                    ));
                    break;
                } else {
                    // Regular binding: bind to (nth value idx)
                    bindings.push((
                        Edn::Symbol(sym.clone()),
                        Edn::List(vec![
                            Edn::Symbol(Symbol::new("nth")),
                            Edn::Symbol(value_sym.clone()),
                            Edn::Number(suss_core::Number::from_i64(idx as i64)),
                        ]),
                    ));
                    idx += 1;
                }
            } else if let Edn::Vector(nested) = elem {
                // Nested destructuring - generate temp symbol and recurse
                let temp_sym = self.gensym("vec");
                // First bind temp to (nth value idx)
                bindings.push((
                    Edn::Symbol(temp_sym.clone()),
                    Edn::List(vec![
                        Edn::Symbol(Symbol::new("nth")),
                        Edn::Symbol(value_sym.clone()),
                        Edn::Number(suss_core::Number::from_i64(idx as i64)),
                    ]),
                ));
                // Then destructure the nested pattern
                let nested_bindings = self.destructure_vector(nested, &temp_sym)?;
                bindings.extend(nested_bindings);
                idx += 1;
            } else {
                return Err(CompileError::MacroExpansion(format!(
                    "Invalid destructuring pattern element: {:?}", elem
                )));
            }
        }

        Ok(bindings)
    }

    /// Generate destructured bindings for a map pattern
    /// Supports:
    ///   {a :key-a, b :key-b}       — direct symbol-to-key mapping
    ///   {:keys [a b]}              — keyword keys matching symbol names
    ///   {:strs [a b]}              — string keys matching symbol names
    ///   {:or {a default, b default}} — default values
    ///   {:as name}                 — bind the whole map
    fn destructure_map(&mut self, pairs: &[(Edn, Edn)], value_sym: &Symbol) -> CompileResult<Vec<(Edn, Edn)>> {
        let mut bindings = Vec::new();
        let mut defaults: Vec<(Edn, Edn)> = Vec::new();
        let mut as_binding = None;

        for (key, val) in pairs {
            match key {
                // {:keys [a b c]} — extract keywords with same name
                Edn::Keyword(kw) if kw.name == "keys" => {
                    if let Edn::Vector(syms) = val {
                        for sym_edn in syms {
                            if let Edn::Symbol(sym) = sym_edn {
                                // (get map :sym-name)
                                bindings.push((
                                    Edn::Symbol(sym.clone()),
                                    Edn::List(vec![
                                        Edn::Symbol(Symbol::new("get")),
                                        Edn::Symbol(value_sym.clone()),
                                        Edn::Keyword(Keyword::new(&sym.name)),
                                    ]),
                                ));
                            } else {
                                return Err(CompileError::MacroExpansion(
                                    ":keys vector must contain symbols".into()
                                ));
                            }
                        }
                    } else {
                        return Err(CompileError::MacroExpansion(
                            ":keys must be followed by a vector".into()
                        ));
                    }
                }
                // {:strs [a b c]} — extract string keys with same name
                Edn::Keyword(kw) if kw.name == "strs" => {
                    if let Edn::Vector(syms) = val {
                        for sym_edn in syms {
                            if let Edn::Symbol(sym) = sym_edn {
                                // (get map "sym-name")
                                bindings.push((
                                    Edn::Symbol(sym.clone()),
                                    Edn::List(vec![
                                        Edn::Symbol(Symbol::new("get")),
                                        Edn::Symbol(value_sym.clone()),
                                        Edn::String(sym.name.clone()),
                                    ]),
                                ));
                            } else {
                                return Err(CompileError::MacroExpansion(
                                    ":strs vector must contain symbols".into()
                                ));
                            }
                        }
                    } else {
                        return Err(CompileError::MacroExpansion(
                            ":strs must be followed by a vector".into()
                        ));
                    }
                }
                // {:or {a default-val, b default-val}} — defaults
                Edn::Keyword(kw) if kw.name == "or" => {
                    if let Edn::Map(or_pairs) = val {
                        defaults = or_pairs.clone();
                    } else {
                        return Err(CompileError::MacroExpansion(
                            ":or must be followed by a map".into()
                        ));
                    }
                }
                // {:as name} — bind the whole map
                Edn::Keyword(kw) if kw.name == "as" => {
                    as_binding = Some(val.clone());
                }
                // {sym :keyword} — direct symbol-to-key binding
                Edn::Symbol(sym) => {
                    bindings.push((
                        Edn::Symbol(sym.clone()),
                        Edn::List(vec![
                            Edn::Symbol(Symbol::new("get")),
                            Edn::Symbol(value_sym.clone()),
                            val.clone(),
                        ]),
                    ));
                }
                _ => {
                    return Err(CompileError::MacroExpansion(format!(
                        "Invalid map destructuring key: {:?}", key
                    )));
                }
            }
        }

        // Apply defaults: wrap existing bindings with (let [sym (if (nil? sym) default sym)])
        if !defaults.is_empty() {
            let mut final_bindings = Vec::new();
            for (sym, expr) in bindings {
                let sym_name = match &sym {
                    Edn::Symbol(s) => s.name.clone(),
                    _ => unreachable!(),
                };
                // Check if there's a default for this symbol
                let has_default = defaults.iter().find(|(k, _)| {
                    matches!(k, Edn::Symbol(s) if s.name == sym_name)
                });
                if let Some((_, default_val)) = has_default {
                    // First bind the raw get, then rebind with default
                    let temp = self.gensym("or");
                    final_bindings.push((Edn::Symbol(temp.clone()), expr));
                    final_bindings.push((
                        sym,
                        Edn::List(vec![
                            Edn::Symbol(Symbol::new("if")),
                            Edn::List(vec![
                                Edn::Symbol(Symbol::new("nil?")),
                                Edn::Symbol(temp.clone()),
                            ]),
                            default_val.clone(),
                            Edn::Symbol(temp),
                        ]),
                    ));
                } else {
                    final_bindings.push((sym, expr));
                }
            }
            bindings = final_bindings;
        }

        // Add :as binding
        if let Some(as_sym) = as_binding {
            bindings.push((as_sym, Edn::Symbol(value_sym.clone())));
        }

        Ok(bindings)
    }

    /// Expand a binding pair, handling destructuring
    /// Returns expanded bindings as a flat vector
    fn expand_binding_pair(&mut self, pattern: Edn, value: Edn) -> CompileResult<Vec<Edn>> {
        if Self::needs_destructuring(&pattern) {
            match pattern {
                Edn::Vector(ref elems) => {
                    let temp_sym = self.gensym("destructure");
                    let mut result = vec![
                        Edn::Symbol(temp_sym.clone()),
                        self.expand(value)?,
                    ];

                    let bindings = self.destructure_vector(elems, &temp_sym)?;
                    for (sym, expr) in bindings {
                        result.push(sym);
                        result.push(self.expand(expr)?);
                    }

                    Ok(result)
                }
                Edn::Map(ref pairs) => {
                    let temp_sym = self.gensym("destructure");
                    let mut result = vec![
                        Edn::Symbol(temp_sym.clone()),
                        self.expand(value)?,
                    ];

                    let bindings = self.destructure_map(pairs, &temp_sym)?;
                    for (sym, expr) in bindings {
                        result.push(sym);
                        result.push(self.expand(expr)?);
                    }

                    Ok(result)
                }
                _ => unreachable!(),
            }
        } else {
            // Simple binding - just expand the value
            Ok(vec![pattern, self.expand(value)?])
        }
    }

    /// Expand a let form, handling destructuring in bindings
    fn expand_let(&mut self, items: &[Edn]) -> CompileResult<Edn> {
        if items.len() < 2 {
            return Err(CompileError::MacroExpansion(
                "let requires bindings and body".into()
            ));
        }

        let bindings = match &items[1] {
            Edn::Vector(v) => v,
            _ => return Err(CompileError::MacroExpansion(
                "let bindings must be a vector".into()
            )),
        };

        if bindings.len() % 2 != 0 {
            return Err(CompileError::MacroExpansion(
                "let bindings must have even number of forms".into()
            ));
        }

        // Process bindings in pairs
        let mut expanded_bindings = Vec::new();
        for chunk in bindings.chunks(2) {
            let expanded = self.expand_binding_pair(chunk[0].clone(), chunk[1].clone())?;
            expanded_bindings.extend(expanded);
        }

        // Build result with expanded bindings and recursively expanded body
        let mut result = vec![
            Edn::Symbol(Symbol::new("let")),
            Edn::Vector(expanded_bindings),
        ];

        // Expand body forms
        for body_form in &items[2..] {
            result.push(self.expand(body_form.clone())?);
        }

        Ok(Edn::List(result))
    }

    /// Check if an Edn item is a multi-arity clause: a List where first element is a Vector
    fn is_arity_clause(item: &Edn) -> bool {
        if let Edn::List(clause) = item {
            matches!(clause.first(), Some(Edn::Vector(_)))
        } else {
            false
        }
    }

    /// Expand an fn form, handling destructuring in parameters
    /// Single-arity: (fn [params] body) or (fn name [params] body)
    /// Multi-arity: (fn ([params1] body1) ([params2] body2)...) or (fn name ([params1] body1)...)
    fn expand_fn(&mut self, items: &[Edn]) -> CompileResult<Edn> {
        if items.len() < 2 {
            return Err(CompileError::MacroExpansion("fn requires parameters".into()));
        }

        // Check for optional name and determine where params/clauses start
        let (name_opt, body_start) = match &items[1] {
            Edn::Symbol(_) => (Some(items[1].clone()), 2),
            Edn::Vector(_) => (None, 1),
            Edn::List(_) => (None, 1), // Could be multi-arity clause
            _ => return Err(CompileError::MacroExpansion(
                "fn requires parameter vector or arity clauses".into()
            )),
        };

        if items.len() <= body_start {
            return Err(CompileError::MacroExpansion(
                "fn requires parameter vector or body".into()
            ));
        }

        // Check for multi-arity: (fn ([p1] b1) ([p2] b2)...) or (fn name ([p1] b1) ([p2] b2)...)
        if Self::is_arity_clause(&items[body_start]) {
            // Multi-arity: pass through, recursively expanding each clause body
            let mut result = vec![Edn::Symbol(Symbol::new("fn"))];
            if let Some(name) = name_opt {
                result.push(name);
            }

            // Process each arity clause
            for clause in &items[body_start..] {
                if let Edn::List(clause_items) = clause {
                    if clause_items.is_empty() {
                        return Err(CompileError::MacroExpansion(
                            "Empty arity clause in fn".into()
                        ));
                    }
                    // Recursively expand the params and body within the clause
                    // Clause format: ([params] body...)
                    let expanded_clause = self.expand_fn_clause(clause_items)?;
                    result.push(Edn::List(expanded_clause));
                } else {
                    return Err(CompileError::MacroExpansion(
                        "Invalid arity clause in multi-arity fn".into()
                    ));
                }
            }

            return Ok(Edn::List(result));
        }

        // Single-arity: params must be a vector
        let params = match &items[body_start] {
            Edn::Vector(v) => v,
            _ => return Err(CompileError::MacroExpansion(
                "fn parameters must be a vector".into()
            )),
        };

        // Check if any parameter needs destructuring
        let has_destructuring = params.iter().any(|p| {
            // Don't treat & as needing destructuring
            if let Edn::Symbol(s) = p {
                if s.name == "&" {
                    return false;
                }
            }
            Self::needs_destructuring(p)
        });

        if !has_destructuring {
            // No destructuring needed - just recursively expand
            let expanded: CompileResult<Vec<Edn>> = items.iter()
                .map(|e| self.expand(e.clone()))
                .collect();
            return Ok(Edn::List(expanded?));
        }

        // Generate new parameter names and collect destructuring bindings
        let mut new_params = Vec::new();
        let mut let_bindings = Vec::new();

        for param in params {
            if let Edn::Symbol(sym) = param {
                if sym.name == "&" {
                    new_params.push(Edn::Symbol(sym.clone()));
                    continue;
                }
            }

            if Self::needs_destructuring(param) {
                let temp_sym = self.gensym("p");
                new_params.push(Edn::Symbol(temp_sym.clone()));

                match param {
                    Edn::Vector(elems) => {
                        let bindings = self.destructure_vector(elems, &temp_sym)?;
                        for (sym, expr) in bindings {
                            let_bindings.push(sym);
                            let_bindings.push(expr);
                        }
                    }
                    Edn::Map(pairs) => {
                        let bindings = self.destructure_map(pairs, &temp_sym)?;
                        for (sym, expr) in bindings {
                            let_bindings.push(sym);
                            let_bindings.push(expr);
                        }
                    }
                    _ => {
                        return Err(CompileError::MacroExpansion(
                            "Invalid destructuring pattern in fn params".into()
                        ));
                    }
                }
            } else {
                new_params.push(param.clone());
            }
        }

        // Build the new fn with let wrapper
        let mut result = vec![Edn::Symbol(Symbol::new("fn"))];
        if let Some(name) = name_opt {
            result.push(name);
        }
        result.push(Edn::Vector(new_params));

        // Wrap body in let if we have destructuring bindings
        if !let_bindings.is_empty() {
            let mut body_forms = Vec::new();
            for body_form in &items[body_start + 1..] {
                body_forms.push(self.expand(body_form.clone())?);
            }

            let let_body = if body_forms.len() == 1 {
                body_forms.pop().unwrap()
            } else {
                let mut do_form = vec![Edn::Symbol(Symbol::new("do"))];
                do_form.extend(body_forms);
                Edn::List(do_form)
            };

            result.push(Edn::List(vec![
                Edn::Symbol(Symbol::new("let")),
                Edn::Vector(let_bindings),
                let_body,
            ]));
        } else {
            for body_form in &items[body_start + 1..] {
                result.push(self.expand(body_form.clone())?);
            }
        }

        Ok(Edn::List(result))
    }

    /// Expand a single arity clause: ([params] body...) -> [params] expanded-body
    /// Handles destructuring within the clause
    fn expand_fn_clause(&mut self, clause_items: &[Edn]) -> CompileResult<Vec<Edn>> {
        if clause_items.is_empty() {
            return Err(CompileError::MacroExpansion("Empty arity clause".into()));
        }

        let params = match &clause_items[0] {
            Edn::Vector(v) => v,
            _ => return Err(CompileError::MacroExpansion(
                "Arity clause must start with parameter vector".into()
            )),
        };

        // Check for destructuring
        let has_destructuring = params.iter().any(|p| {
            if let Edn::Symbol(s) = p {
                if s.name == "&" {
                    return false;
                }
            }
            Self::needs_destructuring(p)
        });

        if !has_destructuring {
            // No destructuring - just expand body forms
            let mut result = vec![clause_items[0].clone()];
            for body_form in &clause_items[1..] {
                result.push(self.expand(body_form.clone())?);
            }
            return Ok(result);
        }

        // Handle destructuring
        let mut new_params = Vec::new();
        let mut let_bindings = Vec::new();

        for param in params {
            if let Edn::Symbol(sym) = param {
                if sym.name == "&" {
                    new_params.push(Edn::Symbol(sym.clone()));
                    continue;
                }
            }

            if Self::needs_destructuring(param) {
                let temp_sym = self.gensym("p");
                new_params.push(Edn::Symbol(temp_sym.clone()));

                match param {
                    Edn::Vector(elems) => {
                        let bindings = self.destructure_vector(elems, &temp_sym)?;
                        for (sym, expr) in bindings {
                            let_bindings.push(sym);
                            let_bindings.push(expr);
                        }
                    }
                    Edn::Map(pairs) => {
                        let bindings = self.destructure_map(pairs, &temp_sym)?;
                        for (sym, expr) in bindings {
                            let_bindings.push(sym);
                            let_bindings.push(expr);
                        }
                    }
                    _ => {}
                }
            } else {
                new_params.push(param.clone());
            }
        }

        // Build result with new params and let-wrapped body
        let mut result = vec![Edn::Vector(new_params)];

        let mut body_forms = Vec::new();
        for body_form in &clause_items[1..] {
            body_forms.push(self.expand(body_form.clone())?);
        }

        let let_body = if body_forms.len() == 1 {
            body_forms.pop().unwrap()
        } else {
            let mut do_form = vec![Edn::Symbol(Symbol::new("do"))];
            do_form.extend(body_forms);
            Edn::List(do_form)
        };

        result.push(Edn::List(vec![
            Edn::Symbol(Symbol::new("let")),
            Edn::Vector(let_bindings),
            let_body,
        ]));

        Ok(result)
    }

    /// Expand a loop form, handling destructuring in bindings
    fn expand_loop(&mut self, items: &[Edn]) -> CompileResult<Edn> {
        if items.len() < 2 {
            return Err(CompileError::MacroExpansion(
                "loop requires bindings and body".into()
            ));
        }

        let bindings = match &items[1] {
            Edn::Vector(v) => v,
            _ => return Err(CompileError::MacroExpansion(
                "loop bindings must be a vector".into()
            )),
        };

        if bindings.len() % 2 != 0 {
            return Err(CompileError::MacroExpansion(
                "loop bindings must have even number of forms".into()
            ));
        }

        // Check if any binding needs destructuring
        let has_destructuring = bindings.chunks(2)
            .any(|chunk| Self::needs_destructuring(&chunk[0]));

        if !has_destructuring {
            // No destructuring - just recursively expand
            let expanded: CompileResult<Vec<Edn>> = items.iter()
                .map(|e| self.expand(e.clone()))
                .collect();
            return Ok(Edn::List(expanded?));
        }

        // For loop with destructuring, we need to:
        // 1. Create simple loop variables
        // 2. Add let bindings inside loop body
        // 3. Transform recur calls to use the simple variables

        // This is complex because recur args must match loop bindings
        // For now, use simpler approach: wrap in let for initial destructuring

        let mut outer_let_bindings = Vec::new();
        let mut loop_bindings = Vec::new();

        for chunk in bindings.chunks(2) {
            let pattern = &chunk[0];
            let value = &chunk[1];

            if Self::needs_destructuring(pattern) {
                let temp_sym = self.gensym("loop_var");
                // Bind temp to initial value
                loop_bindings.push(Edn::Symbol(temp_sym.clone()));
                loop_bindings.push(self.expand(value.clone())?);

                // Will add inner let for destructuring
                match pattern {
                    Edn::Vector(elems) => {
                        let bindings = self.destructure_vector(elems, &temp_sym)?;
                        for (sym, expr) in bindings {
                            outer_let_bindings.push(sym);
                            outer_let_bindings.push(expr);
                        }
                    }
                    _ => {
                        return Err(CompileError::MacroExpansion(
                            "Only vector destructuring supported in loop".into()
                        ));
                    }
                }
            } else {
                loop_bindings.push(pattern.clone());
                loop_bindings.push(self.expand(value.clone())?);
            }
        }

        // Build loop with inner let for destructuring
        let mut body_forms = Vec::new();
        for body_form in &items[2..] {
            body_forms.push(self.expand(body_form.clone())?);
        }

        let body = if body_forms.len() == 1 {
            body_forms.pop().unwrap()
        } else {
            let mut do_form = vec![Edn::Symbol(Symbol::new("do"))];
            do_form.extend(body_forms);
            Edn::List(do_form)
        };

        let inner_body = if !outer_let_bindings.is_empty() {
            Edn::List(vec![
                Edn::Symbol(Symbol::new("let")),
                Edn::Vector(outer_let_bindings),
                body,
            ])
        } else {
            body
        };

        Ok(Edn::List(vec![
            Edn::Symbol(Symbol::new("loop")),
            Edn::Vector(loop_bindings),
            inner_body,
        ]))
    }

    /// Parse for bindings into groups
    /// Each group is: (binding_sym, collection_expr, when_clauses, let_bindings)
    fn parse_for_bindings(&self, bindings: &[Edn]) -> CompileResult<Vec<ForBindingGroup>> {
        let mut groups = Vec::new();
        let mut i = 0;

        while i < bindings.len() {
            // Expect a binding pattern
            let pattern = bindings[i].clone();
            i += 1;

            if i >= bindings.len() {
                return Err(CompileError::MacroExpansion(
                    "for binding requires collection expression".into()
                ));
            }

            // Check if next is a keyword modifier or collection
            let mut collection = None;
            let mut when_clauses = Vec::new();
            let mut let_bindings = Vec::new();

            while i < bindings.len() {
                match &bindings[i] {
                    Edn::Keyword(kw) if kw.name == "when" => {
                        i += 1;
                        if i >= bindings.len() {
                            return Err(CompileError::MacroExpansion(
                                ":when requires predicate expression".into()
                            ));
                        }
                        when_clauses.push(bindings[i].clone());
                        i += 1;
                    }
                    Edn::Keyword(kw) if kw.name == "let" => {
                        i += 1;
                        if i >= bindings.len() {
                            return Err(CompileError::MacroExpansion(
                                ":let requires binding vector".into()
                            ));
                        }
                        match &bindings[i] {
                            Edn::Vector(v) => {
                                let_bindings.extend(v.clone());
                                i += 1;
                            }
                            _ => return Err(CompileError::MacroExpansion(
                                ":let requires binding vector".into()
                            )),
                        }
                    }
                    Edn::Keyword(kw) if kw.name == "while" => {
                        // :while is similar to :when but breaks iteration
                        // For simplicity, treat it like :when for now
                        i += 1;
                        if i >= bindings.len() {
                            return Err(CompileError::MacroExpansion(
                                ":while requires predicate expression".into()
                            ));
                        }
                        when_clauses.push(bindings[i].clone());
                        i += 1;
                    }
                    _ if collection.is_none() => {
                        // This is the collection expression
                        collection = Some(bindings[i].clone());
                        i += 1;
                    }
                    _ => {
                        // Start of next binding group
                        break;
                    }
                }
            }

            let coll = collection.ok_or_else(|| {
                CompileError::MacroExpansion("for binding requires collection expression".into())
            })?;

            groups.push(ForBindingGroup {
                pattern,
                collection: coll,
                when_clauses,
                let_bindings,
            });
        }

        Ok(groups)
    }

    /// Expand a for form (list comprehension)
    fn expand_for(&mut self, items: &[Edn]) -> CompileResult<Edn> {
        if items.len() < 3 {
            return Err(CompileError::MacroExpansion(
                "for requires bindings and body".into()
            ));
        }

        let bindings = match &items[1] {
            Edn::Vector(v) => v,
            _ => return Err(CompileError::MacroExpansion(
                "for bindings must be a vector".into()
            )),
        };

        let body = items[2].clone();
        let groups = self.parse_for_bindings(bindings)?;

        if groups.is_empty() {
            return Err(CompileError::MacroExpansion(
                "for requires at least one binding".into()
            ));
        }

        // Build the expansion from innermost to outermost
        self.expand_for_groups(&groups, body)
    }

    /// Recursively expand for binding groups
    fn expand_for_groups(&mut self, groups: &[ForBindingGroup], body: Edn) -> CompileResult<Edn> {
        if groups.is_empty() {
            return self.expand(body);
        }

        let group = &groups[0];
        let rest_groups = &groups[1..];

        // Build the inner expression
        let inner = if rest_groups.is_empty() {
            // Innermost - just the body
            body
        } else {
            // Build nested for for remaining groups
            let mut inner_bindings = Vec::new();
            for g in rest_groups {
                inner_bindings.push(g.pattern.clone());
                inner_bindings.push(g.collection.clone());
                for when in &g.when_clauses {
                    inner_bindings.push(Edn::Keyword(Keyword::new("when")));
                    inner_bindings.push(when.clone());
                }
                if !g.let_bindings.is_empty() {
                    inner_bindings.push(Edn::Keyword(Keyword::new("let")));
                    inner_bindings.push(Edn::Vector(g.let_bindings.clone()));
                }
            }
            Edn::List(vec![
                Edn::Symbol(Symbol::new("for")),
                Edn::Vector(inner_bindings),
                body,
            ])
        };

        // Wrap in :let bindings if any
        let with_let = if group.let_bindings.is_empty() {
            inner
        } else {
            Edn::List(vec![
                Edn::Symbol(Symbol::new("let")),
                Edn::Vector(group.let_bindings.clone()),
                inner,
            ])
        };

        // Build the fn that takes the binding pattern
        let fn_body = with_let;
        let fn_param = group.pattern.clone();

        // If there are :when clauses, wrap collection in filter
        let filtered_coll = if group.when_clauses.is_empty() {
            group.collection.clone()
        } else {
            // Build (filter (fn [pattern] (and when1 when2 ...)) coll)
            let pred = if group.when_clauses.len() == 1 {
                group.when_clauses[0].clone()
            } else {
                let mut and_form = vec![Edn::Symbol(Symbol::new("and"))];
                and_form.extend(group.when_clauses.clone());
                Edn::List(and_form)
            };

            Edn::List(vec![
                Edn::Symbol(Symbol::new("filter")),
                Edn::List(vec![
                    Edn::Symbol(Symbol::new("fn")),
                    Edn::Vector(vec![fn_param.clone()]),
                    pred,
                ]),
                group.collection.clone(),
            ])
        };

        // Choose map or mapcat based on whether there are more groups
        let map_fn = if rest_groups.is_empty() {
            Symbol::new("map")
        } else {
            Symbol::new("mapcat")
        };

        let result = Edn::List(vec![
            Edn::Symbol(map_fn),
            Edn::List(vec![
                Edn::Symbol(Symbol::new("fn")),
                Edn::Vector(vec![fn_param]),
                fn_body,
            ]),
            filtered_coll,
        ]);

        self.expand(result)
    }

    /// Expand an expression, handling syntax-quote and macro calls
    pub fn expand(&mut self, expr: Edn) -> CompileResult<Edn> {
        match expr {
            // Handle syntax-quote
            Edn::List(ref items) if !items.is_empty() => {
                if let Edn::Symbol(sym) = &items[0] {
                    match sym.name.as_str() {
                        "syntax-quote" => {
                            if items.len() != 2 {
                                return Err(CompileError::MacroExpansion(
                                    "syntax-quote requires exactly one argument".into(),
                                ));
                            }
                            return self.expand_syntax_quote(&items[1]);
                        }
                        "quote" => {
                            // Don't expand inside quote
                            return Ok(expr);
                        }
                        "unquote" | "unquote-splicing" => {
                            return Err(CompileError::MacroExpansion(format!(
                                "{} outside of syntax-quote",
                                sym.name
                            )));
                        }
                        // Handle special forms with destructuring
                        "let" => {
                            return self.expand_let(items);
                        }
                        "fn" => {
                            return self.expand_fn(items);
                        }
                        "loop" => {
                            return self.expand_loop(items);
                        }
                        "for" => {
                            return self.expand_for(items);
                        }
                        name => {
                            // Check for macro call
                            if self.is_macro(name) {
                                let expanded = self.expand_macro_call(name, &items[1..])?;
                                // Recursively expand the result
                                return self.expand(expanded);
                            }
                        }
                    }
                }

                // Recursively expand list elements
                let expanded: CompileResult<Vec<Edn>> =
                    items.iter().map(|e| self.expand(e.clone())).collect();
                Ok(Edn::List(expanded?))
            }

            // Recursively expand vectors
            Edn::Vector(items) => {
                let expanded: CompileResult<Vec<Edn>> =
                    items.into_iter().map(|e| self.expand(e)).collect();
                Ok(Edn::Vector(expanded?))
            }

            // Recursively expand maps
            Edn::Map(pairs) => {
                let expanded: CompileResult<Vec<(Edn, Edn)>> = pairs
                    .into_iter()
                    .map(|(k, v)| Ok((self.expand(k)?, self.expand(v)?)))
                    .collect();
                Ok(Edn::Map(expanded?))
            }

            // Recursively expand sets
            Edn::Set(items) => {
                let expanded: CompileResult<Vec<Edn>> =
                    items.into_iter().map(|e| self.expand(e)).collect();
                Ok(Edn::Set(expanded?))
            }

            // Atoms pass through
            _ => Ok(expr),
        }
    }

    /// Expand a syntax-quote form
    fn expand_syntax_quote(&mut self, expr: &Edn) -> CompileResult<Edn> {
        self.expand_syntax_quote_inner(expr, true)
    }

    /// Inner syntax-quote expansion with auto-gensym tracking
    fn expand_syntax_quote_inner(&mut self, expr: &Edn, toplevel: bool) -> CompileResult<Edn> {
        match expr {
            // Handle unquote: ~x
            Edn::List(items) if !items.is_empty() => {
                if let Edn::Symbol(sym) = &items[0] {
                    if sym.name == "unquote" {
                        if items.len() != 2 {
                            return Err(CompileError::MacroExpansion(
                                "unquote requires exactly one argument".into(),
                            ));
                        }
                        // Return the form as-is (will be evaluated)
                        return Ok(items[1].clone());
                    }
                    if sym.name == "unquote-splicing" {
                        return Err(CompileError::MacroExpansion(
                            "unquote-splicing not at sequence position".into(),
                        ));
                    }
                }

                // Process list with potential splicing
                self.expand_syntax_quote_list(items)
            }

            // Handle vector with potential splicing
            Edn::Vector(items) => {
                let expanded = self.expand_syntax_quote_seq(items)?;
                // Wrap in (vec ...) or (apply vector (concat ...))
                Ok(Edn::List(vec![
                    Edn::Symbol(Symbol::new("apply")),
                    Edn::Symbol(Symbol::new("vector")),
                    expanded,
                ]))
            }

            // Handle symbol: potentially auto-gensym foo#
            Edn::Symbol(sym) => {
                let name = sym.name.as_str();
                if name.ends_with('#') {
                    // Auto-gensym: foo# -> foo__N__auto__
                    let base = &name[..name.len() - 1];
                    let gensym = self.gensym(base);
                    Ok(Edn::List(vec![
                        Edn::Symbol(Symbol::new("quote")),
                        Edn::Symbol(gensym),
                    ]))
                } else if !name.contains('/') && self.current_ns.is_some() {
                    // Namespace-qualify unqualified symbols
                    let ns = self.current_ns.as_ref().unwrap();
                    let qualified = Symbol::namespaced(ns, name);
                    Ok(Edn::List(vec![
                        Edn::Symbol(Symbol::new("quote")),
                        Edn::Symbol(qualified),
                    ]))
                } else {
                    // Already qualified or no namespace
                    Ok(Edn::List(vec![
                        Edn::Symbol(Symbol::new("quote")),
                        Edn::Symbol(sym.clone()),
                    ]))
                }
            }

            // Keywords, numbers, strings, etc. are self-quoting
            Edn::Keyword(_) | Edn::Number(_) | Edn::String(_) | Edn::Bool(_) | Edn::Nil => {
                Ok(expr.clone())
            }

            // Handle maps
            Edn::Map(pairs) => {
                let mut args = vec![Edn::Symbol(Symbol::new("hash-map"))];
                for (k, v) in pairs {
                    args.push(self.expand_syntax_quote_inner(k, false)?);
                    args.push(self.expand_syntax_quote_inner(v, false)?);
                }
                Ok(Edn::List(args))
            }

            // Handle sets
            Edn::Set(items) => {
                let mut args = vec![Edn::Symbol(Symbol::new("hash-set"))];
                for item in items {
                    args.push(self.expand_syntax_quote_inner(item, false)?);
                }
                Ok(Edn::List(args))
            }

            // Other forms pass through quoted
            _ => Ok(Edn::List(vec![
                Edn::Symbol(Symbol::new("quote")),
                expr.clone(),
            ])),
        }
    }

    /// Expand a syntax-quoted list, handling splicing
    fn expand_syntax_quote_list(&mut self, items: &[Edn]) -> CompileResult<Edn> {
        let expanded = self.expand_syntax_quote_seq(items)?;
        // Wrap in (apply list (concat ...))
        Ok(Edn::List(vec![
            Edn::Symbol(Symbol::new("apply")),
            Edn::Symbol(Symbol::new("list")),
            expanded,
        ]))
    }

    /// Expand a sequence for syntax-quote, returning a (concat ...) form
    fn expand_syntax_quote_seq(&mut self, items: &[Edn]) -> CompileResult<Edn> {
        let mut concat_args = vec![Edn::Symbol(Symbol::new("concat"))];

        for item in items {
            if let Edn::List(inner) = item {
                if let Some(Edn::Symbol(sym)) = inner.first() {
                    if sym.name == "unquote-splicing" {
                        if inner.len() != 2 {
                            return Err(CompileError::MacroExpansion(
                                "unquote-splicing requires exactly one argument".into(),
                            ));
                        }
                        // ~@x -> x (spliced directly)
                        concat_args.push(inner[1].clone());
                        continue;
                    }
                    if sym.name == "unquote" {
                        if inner.len() != 2 {
                            return Err(CompileError::MacroExpansion(
                                "unquote requires exactly one argument".into(),
                            ));
                        }
                        // ~x -> (list x)
                        concat_args.push(Edn::List(vec![
                            Edn::Symbol(Symbol::new("list")),
                            inner[1].clone(),
                        ]));
                        continue;
                    }
                }
            }

            // Regular element: wrap in list after recursive expansion
            let expanded = self.expand_syntax_quote_inner(item, false)?;
            concat_args.push(Edn::List(vec![
                Edn::Symbol(Symbol::new("list")),
                expanded,
            ]));
        }

        Ok(Edn::List(concat_args))
    }

    /// Expand a macro call
    fn expand_macro_call(&mut self, name: &str, args: &[Edn]) -> CompileResult<Edn> {
        let macro_def = self.macros.get(name).cloned().ok_or_else(|| {
            CompileError::MacroExpansion(format!("Undefined macro: {}", name))
        })?;

        // Create environment for macro execution
        let mut env = Env::new();
        env.push_scope();

        // Bind parameters
        if args.len() < macro_def.params.len() {
            return Err(CompileError::MacroExpansion(format!(
                "Macro {} expects at least {} arguments, got {}",
                name,
                macro_def.params.len(),
                args.len()
            )));
        }

        // Bind regular parameters
        for (param, arg) in macro_def.params.iter().zip(args.iter()) {
            env.define(param, arg.clone());
        }

        // Bind rest parameter if present
        if let Some(ref rest_name) = macro_def.rest_param {
            let rest_args: Vec<Edn> = args[macro_def.params.len()..].to_vec();
            env.define(rest_name, Edn::List(rest_args));
        } else if args.len() > macro_def.params.len() {
            return Err(CompileError::MacroExpansion(format!(
                "Macro {} expects {} arguments, got {}",
                name,
                macro_def.params.len(),
                args.len()
            )));
        }

        // Evaluate the macro body
        self.evaluator.eval(&macro_def.body, &mut env)
    }
}

impl Default for MacroEnv {
    fn default() -> Self {
        Self::new()
    }
}

/// Top-level function to expand all expressions
pub fn expand_all(exprs: Vec<Edn>, current_ns: Option<&str>) -> CompileResult<Vec<Edn>> {
    let mut env = MacroEnv::new();
    if let Some(ns) = current_ns {
        env.set_namespace(ns);
    }
    let result = env.expand_all(exprs)?;

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Edn {
        let mut state = suss_reader::ParserState::new("suss");
        suss_reader::parse(s, &mut state).unwrap()
    }

    fn parse_all(s: &str) -> Vec<Edn> {
        let mut state = suss_reader::ParserState::new("suss");
        suss_reader::parse_all(s, &mut state).unwrap()
    }

    #[test]
    fn test_gensym() {
        let mut env = MacroEnv::new();
        let s1 = env.gensym("foo");
        let s2 = env.gensym("foo");
        assert_ne!(s1.name, s2.name);
        assert!(s1.name.starts_with("foo__"));
        assert!(s1.name.ends_with("__auto__"));
    }

    #[test]
    fn test_expand_simple_quote() {
        let mut env = MacroEnv::new();
        let expr = parse("(quote (a b c))");
        let expanded = env.expand(expr.clone()).unwrap();
        // quote should pass through unchanged
        assert_eq!(expanded, expr);
    }

    #[test]
    fn test_defmacro_simple() {
        let mut env = MacroEnv::new();
        let exprs = parse_all("(defmacro m1 [x] x)");
        let result = env.expand_all(exprs).unwrap();
        // defmacro is consumed, not emitted
        assert!(result.is_empty());
        assert!(env.is_macro("m1"));
    }

    #[test]
    fn test_macro_call() {
        let mut env = MacroEnv::new();
        // Define a simple identity macro
        let _ = env.expand_all(parse_all("(defmacro identity [x] x)")).unwrap();

        // Call the macro
        let result = env.expand(parse("(identity 42)")).unwrap();
        assert_eq!(result, Edn::Number(suss_core::Number::from_i64(42)));
    }

    #[test]
    fn test_macro_with_rest_params() {
        let mut env = MacroEnv::new();
        // Define a macro that returns its args as a list
        let _ = env.expand_all(parse_all("(defmacro wrap [& body] (cons 'do body))")).unwrap();

        // Call with multiple args
        let result = env.expand(parse("(wrap 1 2 3)")).unwrap();
        // Should produce (do 1 2 3)
        if let Edn::List(items) = result {
            assert_eq!(items.len(), 4);
            if let Edn::Symbol(s) = &items[0] {
                assert_eq!(s.name, "do");
            } else {
                panic!("Expected symbol 'do'");
            }
        } else {
            panic!("Expected list");
        }
    }

    #[test]
    fn test_thread_first_simple() {
        let mut env = MacroEnv::new();
        // (-> 1 inc) should expand to (inc 1)
        let result = env.expand(parse("(-> 1 inc)")).unwrap();
        if let Edn::List(items) = result {
            assert_eq!(items.len(), 2);
            if let Edn::Symbol(s) = &items[0] {
                assert_eq!(s.name, "inc");
            }
            assert!(matches!(&items[1], Edn::Number(_)));
        } else {
            panic!("Expected list");
        }
    }

    #[test]
    fn test_thread_first_with_args() {
        let mut env = MacroEnv::new();
        // (-> 1 (+ 2)) should expand to (+ 1 2)
        let result = env.expand(parse("(-> 1 (+ 2))")).unwrap();
        if let Edn::List(items) = result {
            assert_eq!(items.len(), 3);
            if let Edn::Symbol(s) = &items[0] {
                assert_eq!(s.name, "+");
            }
        } else {
            panic!("Expected list");
        }
    }

    #[test]
    fn test_thread_first_chained() {
        let mut env = MacroEnv::new();
        // (-> 1 (+ 2) (* 3)) should expand to (* (+ 1 2) 3)
        let result = env.expand(parse("(-> 1 (+ 2) (* 3))")).unwrap();
        if let Edn::List(items) = result {
            assert_eq!(items.len(), 3);
            if let Edn::Symbol(s) = &items[0] {
                assert_eq!(s.name, "*");
            }
            // Second arg should be (+ 1 2)
            if let Edn::List(inner) = &items[1] {
                if let Edn::Symbol(s) = &inner[0] {
                    assert_eq!(s.name, "+");
                }
            }
        } else {
            panic!("Expected list");
        }
    }

    #[test]
    fn test_thread_last_simple() {
        let mut env = MacroEnv::new();
        // (->> 1 inc) should expand to (inc 1)
        let result = env.expand(parse("(->> 1 inc)")).unwrap();
        if let Edn::List(items) = result {
            assert_eq!(items.len(), 2);
            if let Edn::Symbol(s) = &items[0] {
                assert_eq!(s.name, "inc");
            }
        } else {
            panic!("Expected list");
        }
    }

    #[test]
    fn test_thread_last_with_args() {
        let mut env = MacroEnv::new();
        // (->> 1 (+ 2 3)) should expand to (+ 2 3 1)
        let result = env.expand(parse("(->> 1 (+ 2 3))")).unwrap();
        if let Edn::List(items) = result {
            assert_eq!(items.len(), 4); // + 2 3 1
            if let Edn::Symbol(s) = &items[0] {
                assert_eq!(s.name, "+");
            }
            // Last arg should be 1
            assert!(matches!(&items[3], Edn::Number(n) if n.to_i64() == Some(1)));
        } else {
            panic!("Expected list");
        }
    }

    #[test]
    fn test_when_let_expansion() {
        let mut env = MacroEnv::new();
        // (when-let [x 42] x) should expand to a let/when form
        let result = env.expand(parse("(when-let [x 42] x)")).unwrap();
        // Should be (let [temp expr] (when temp (let [x temp] (do x))))
        if let Edn::List(items) = result {
            if let Edn::Symbol(s) = &items[0] {
                assert_eq!(s.name, "let");
            } else {
                panic!("Expected let");
            }
        } else {
            panic!("Expected list");
        }
    }

    #[test]
    fn test_if_let_expansion() {
        let mut env = MacroEnv::new();
        // (if-let [x 42] x 0) should expand to a let/if form
        let result = env.expand(parse("(if-let [x 42] x 0)")).unwrap();
        // Should be (let [temp expr] (if temp (let [x temp] then) else))
        if let Edn::List(items) = result {
            if let Edn::Symbol(s) = &items[0] {
                assert_eq!(s.name, "let");
            } else {
                panic!("Expected let");
            }
        } else {
            panic!("Expected list");
        }
    }
}
