//! Suss Evaluator - Direct interpreter for Suss expressions
//!
//! This crate provides a tree-walking interpreter for Suss.
//! Eventually this will be replaced by compilation to WASM.

mod eval;
mod primitives;

#[cfg(all(feature = "component", target_family = "wasm"))]
mod component;

pub use suss_core::{Env, Interner, Sexp};
pub use eval::{eval, EvalError};

/// The Suss runtime, holding environment and interner
pub struct Runtime {
    pub env: Env,
    pub interner: Interner,
}

impl Runtime {
    /// Create a new runtime with primitives loaded
    pub fn new() -> Self {
        let mut interner = Interner::new();
        let env = primitives::load_primitives(&mut interner);
        Self { env, interner }
    }

    /// Evaluate an expression in this runtime
    pub fn eval(&mut self, expr: &Sexp) -> Result<Sexp, EvalError> {
        eval(expr, &mut self.env, &mut self.interner)
    }

    /// Parse and evaluate a string
    pub fn eval_string(&mut self, input: &str) -> Result<Sexp, String> {
        let mut state = suss_reader::ParserState::new("suss");
        state.interner = std::mem::take(&mut self.interner);

        // Use parse_and_intern to properly convert placeholder strings to symbols/keywords
        let expr = suss_reader::parse_and_intern(input, &mut state).map_err(|e| e.to_string())?;

        self.interner = state.interner;
        self.eval(&expr).map_err(|e| e.to_string())
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}
