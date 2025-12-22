//! Environment for variable bindings

use crate::Edn;
use std::collections::HashMap;

/// A scope in the environment chain
#[derive(Debug, Clone, PartialEq)]
struct Scope {
    bindings: HashMap<String, Edn>,
}

/// The evaluation environment
#[derive(Debug, Clone, PartialEq)]
pub struct Env {
    scopes: Vec<Scope>,
}

impl Env {
    /// Create an empty environment
    pub fn new() -> Self {
        Self {
            scopes: vec![Scope {
                bindings: HashMap::new(),
            }],
        }
    }

    /// Push a new scope
    pub fn push_scope(&mut self) {
        self.scopes.push(Scope {
            bindings: HashMap::new(),
        });
    }

    /// Pop the current scope
    pub fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    /// Define a binding in the current scope
    pub fn define(&mut self, name: impl Into<String>, value: Edn) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.bindings.insert(name.into(), value);
        }
    }

    /// Define a binding in the global (root) scope
    pub fn define_global(&mut self, name: impl Into<String>, value: Edn) {
        if let Some(scope) = self.scopes.first_mut() {
            scope.bindings.insert(name.into(), value);
        }
    }

    /// Look up a binding by name, searching from innermost to outermost scope
    pub fn lookup(&self, name: &str) -> Option<&Edn> {
        for scope in self.scopes.iter().rev() {
            if let Some(value) = scope.bindings.get(name) {
                return Some(value);
            }
        }
        None
    }

    /// Set a binding (must already exist)
    pub fn set(&mut self, name: &str, value: Edn) -> bool {
        for scope in self.scopes.iter_mut().rev() {
            if scope.bindings.contains_key(name) {
                scope.bindings.insert(name.to_string(), value);
                return true;
            }
        }
        false
    }
}

impl Default for Env {
    fn default() -> Self {
        Self::new()
    }
}
