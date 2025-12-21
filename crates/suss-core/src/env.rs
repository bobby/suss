//! Environment for variable bindings

use crate::{Sexp, SymbolId};
use std::collections::HashMap;

/// A scope in the environment chain
#[derive(Debug, Clone)]
struct Scope {
    bindings: HashMap<SymbolId, Sexp>,
}

/// The evaluation environment
#[derive(Debug, Clone)]
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
    pub fn define(&mut self, name: SymbolId, value: Sexp) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.bindings.insert(name, value);
        }
    }

    /// Define a binding in the global (root) scope
    pub fn define_global(&mut self, name: SymbolId, value: Sexp) {
        if let Some(scope) = self.scopes.first_mut() {
            scope.bindings.insert(name, value);
        }
    }

    /// Look up a binding by name, searching from innermost to outermost scope
    pub fn lookup(&self, name: SymbolId) -> Option<&Sexp> {
        for scope in self.scopes.iter().rev() {
            if let Some(value) = scope.bindings.get(&name) {
                return Some(value);
            }
        }
        None
    }

    /// Set a binding (must already exist)
    pub fn set(&mut self, name: SymbolId, value: Sexp) -> bool {
        for scope in self.scopes.iter_mut().rev() {
            if scope.bindings.contains_key(&name) {
                scope.bindings.insert(name, value);
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
