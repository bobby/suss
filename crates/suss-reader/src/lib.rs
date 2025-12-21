//! Suss Reader - S-expression parser for the Suss language
//!
//! This crate provides a chumsky-based parser for Suss source code,
//! supporting the full EDN data literal syntax plus Suss extensions.

mod parser;

#[cfg(all(feature = "component", target_family = "wasm"))]
mod component;

// Re-export core types
pub use suss_core::{print_sexp, Env, Interner, KeywordId, Number, Sexp, SymbolId};

pub use parser::{parse, parse_all, ParseError, ParserState};

/// Re-export span type
pub use chumsky::span::SimpleSpan as Span;

/// Intern symbols and keywords in an s-expression tree.
///
/// The parser returns placeholder strings like "sym:foo" for symbols
/// and ":bar" for keywords. This function walks the tree and replaces
/// them with proper interned Symbol/Keyword variants.
pub fn intern_sexp(sexp: Sexp, interner: &mut Interner) -> Sexp {
    match sexp {
        Sexp::String(ref s) if s.starts_with("sym:") => {
            Sexp::Symbol(interner.intern_symbol(&s[4..]))
        }
        Sexp::String(ref s) if s.starts_with(":") => {
            Sexp::Keyword(interner.intern_keyword(&s[1..]))
        }
        Sexp::List(items) => {
            Sexp::List(items.into_iter().map(|s| intern_sexp(s, interner)).collect())
        }
        Sexp::Vector(items) => {
            Sexp::Vector(items.into_iter().map(|s| intern_sexp(s, interner)).collect())
        }
        Sexp::Set(items) => {
            Sexp::Set(items.into_iter().map(|s| intern_sexp(s, interner)).collect())
        }
        Sexp::Map(pairs) => {
            Sexp::Map(
                pairs
                    .into_iter()
                    .map(|(k, v)| (intern_sexp(k, interner), intern_sexp(v, interner)))
                    .collect(),
            )
        }
        // ReaderConditional already has KeywordIds from parsing - but values need interning
        Sexp::ReaderConditional(clauses) => {
            Sexp::ReaderConditional(
                clauses
                    .into_iter()
                    .map(|(kw, expr)| (kw, intern_sexp(expr, interner)))
                    .collect(),
            )
        }
        other => other,
    }
}

/// Parse and intern in one step.
///
/// This is the main entry point for parsing Suss code.
pub fn parse_and_intern(input: &str, state: &mut ParserState) -> Result<Sexp, ParseError> {
    let raw = parse(input, state)?;
    Ok(intern_sexp(raw, &mut state.interner))
}

/// Parse all expressions and intern them.
pub fn parse_all_and_intern(input: &str, state: &mut ParserState) -> Result<Vec<Sexp>, ParseError> {
    let raw = parse_all(input, state)?;
    Ok(raw.into_iter().map(|s| intern_sexp(s, &mut state.interner)).collect())
}
