//! Suss Reader - S-expression parser for the Suss language
//!
//! This crate provides a chumsky-based parser for Suss source code,
//! supporting the full EDN data literal syntax plus Suss extensions.

mod parser;

#[cfg(all(feature = "component", target_family = "wasm"))]
mod component;

// Re-export core types
pub use suss_core::{Edn, Env, Keyword, Number, Symbol};

pub use parser::{parse, parse_all, ParseError, ParserState};

/// Re-export span type
pub use chumsky::span::SimpleSpan as Span;
