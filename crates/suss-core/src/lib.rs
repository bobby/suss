//! Suss Core - Shared types for the Suss language
//!
//! This crate contains the core types used across the Suss crates:
//! - Edn: EDN values (the unified value type)
//! - Env: Evaluation environment
//! - Number: Numeric tower (BigInt, Ratio, Float)

mod edn;
mod env;
mod number;

// EDN types
pub use edn::{Edn, Keyword, Symbol, Tagged};

// Core types
pub use env::Env;
pub use number::Number;
