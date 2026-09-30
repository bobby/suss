//! Suss Reader - S-expression parser for the Suss language
//!
//! `forms` provides portable source syntax with byte spans, binary64 and UTF-16.
//! The legacy chumsky EDN API remains a prototype boundary pending compiler
//! migration; neither API claims full upstream reader support.

mod parser;
pub mod forms;

#[cfg(all(feature = "component", target_family = "wasm"))]
mod component;

// Re-export core types
pub use suss_core::{Edn, Env, Keyword, Number, Symbol};

pub use parser::{parse, parse_all, ParseError, ParserState};

/// Re-export span type
pub use chumsky::span::SimpleSpan as Span;
