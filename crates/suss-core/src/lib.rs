//! Suss Core - Shared types for the Suss language
//!
//! This crate contains the core types used across the Suss crates:
//! - Sexp: S-expression values
//! - Env: Evaluation environment
//! - Number: Numeric tower (BigInt, Ratio, Float)
//! - Interner: Symbol and keyword interning

mod env;
mod intern;
mod number;
mod sexp;

#[cfg(feature = "component")]
pub mod wit_convert;

pub use env::Env;
pub use intern::{Interner, KeywordId, SymbolId};
pub use number::Number;
pub use sexp::{print_sexp, Sexp};
