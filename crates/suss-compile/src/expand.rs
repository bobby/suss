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
    /// (defn name [params] body...)
    /// (defn ^:export name [params] body...)
    /// (defn name "docstring" [params] body...)
    fn expand_defn(&self, items: &[Edn]) -> CompileResult<Edn> {
        // defn requires at least: defn name [params] body
        if items.len() < 4 {
            return Err(CompileError::MacroExpansion(
                "defn requires name, params, and body".into(),
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

        // Skip optional docstring
        if idx < items.len() {
            if matches!(&items[idx], Edn::String(_)) {
                idx += 1;
            }
        }

        // Params must be a vector
        if idx >= items.len() {
            return Err(CompileError::MacroExpansion(
                "defn requires parameters vector".into(),
            ));
        }
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

        // Build (def [metadata...] name (fn ...)) form
        let mut def_form = vec![Edn::Symbol(Symbol::new("def"))];
        def_form.extend(metadata);
        def_form.push(name);
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
    env.expand_all(exprs)
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
