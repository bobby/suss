//! WASM Component Model bindings for suss-reader
//!
//! This module implements the reader WIT interface using resource handles.

use std::cell::RefCell;

wit_bindgen::generate!({
    world: "reader-world",
    path: "../../wit",
});

use crate::{parse, parse_all, ParseError as InternalParseError, ParserState};
use suss_core::{Edn, Number as InternalNumber, Symbol, Keyword};
use num_bigint::BigInt;

// Types from the exported types interface
use exports::suss::lang::types::{GuestSexp, ParseError, Span, Sexp as WitSexp, Number as WitNumber, Bigint};
// Reader guest trait
use exports::suss::lang::reader::Guest as ReaderGuest;
use exports::suss::lang::types::Guest as TypesGuest;

// Global state for the reader component
thread_local! {
    static STATE: RefCell<ParserState> = RefCell::new(ParserState::new("suss"));
    // Simple symbol/keyword interning for component compatibility
    static SYMBOLS: RefCell<Vec<String>> = RefCell::new(Vec::new());
    static KEYWORDS: RefCell<Vec<String>> = RefCell::new(Vec::new());
}

/// Resource wrapper for Edn
/// This is an opaque handle that owns the parsed expression
pub struct SexpResource {
    inner: Edn,
}

impl GuestSexp for SexpResource {
    fn to_string(&self) -> String {
        // Edn implements Display for EDN representation
        format!("{}", self.inner)
    }

    fn is_nil(&self) -> bool {
        matches!(self.inner, Edn::Nil)
    }

    fn is_truthy(&self) -> bool {
        !matches!(self.inner, Edn::Nil | Edn::Bool(false))
    }
}

struct Component;

// Implement Guest for the types interface (provides the Sexp resource)
impl TypesGuest for Component {
    type Sexp = SexpResource;
}

// Implement Guest for the reader interface
impl ReaderGuest for Component {
    fn read_string(input: String, platform: String) -> Result<WitSexp, ParseError> {
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.platform = platform;

            match parse(&input, &mut state) {
                Ok(edn) => Ok(WitSexp::new(SexpResource { inner: edn })),
                Err(e) => Err(internal_error_to_wit(e)),
            }
        })
    }

    fn read_all(input: String, platform: String) -> Result<Vec<WitSexp>, ParseError> {
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.platform = platform;

            match parse_all(&input, &mut state) {
                Ok(edns) => Ok(edns
                    .into_iter()
                    .map(|edn| WitSexp::new(SexpResource { inner: edn }))
                    .collect()),
                Err(e) => Err(internal_error_to_wit(e)),
            }
        })
    }

    fn intern_symbol(name: String) -> u32 {
        SYMBOLS.with(|symbols| {
            let mut symbols = symbols.borrow_mut();
            // Check if already interned
            if let Some(idx) = symbols.iter().position(|s| s == &name) {
                return idx as u32;
            }
            // Add new symbol
            let idx = symbols.len() as u32;
            symbols.push(name);
            idx
        })
    }

    fn symbol_name(id: u32) -> String {
        SYMBOLS.with(|symbols| {
            let symbols = symbols.borrow();
            symbols.get(id as usize).cloned().unwrap_or_default()
        })
    }

    fn intern_keyword(name: String) -> u32 {
        KEYWORDS.with(|keywords| {
            let mut keywords = keywords.borrow_mut();
            // Check if already interned
            if let Some(idx) = keywords.iter().position(|k| k == &name) {
                return idx as u32;
            }
            // Add new keyword
            let idx = keywords.len() as u32;
            keywords.push(name);
            idx
        })
    }

    fn keyword_name(id: u32) -> String {
        KEYWORDS.with(|keywords| {
            let keywords = keywords.borrow();
            keywords.get(id as usize).cloned().unwrap_or_default()
        })
    }

    // === Factory functions ===

    fn make_nil() -> WitSexp {
        WitSexp::new(SexpResource { inner: Edn::Nil })
    }

    fn make_bool(b: bool) -> WitSexp {
        WitSexp::new(SexpResource { inner: Edn::Bool(b) })
    }

    fn make_number(n: WitNumber) -> WitSexp {
        WitSexp::new(SexpResource { inner: Edn::Number(wit_to_internal_number(n)) })
    }

    fn make_symbol(id: u32) -> WitSexp {
        // Look up the symbol name and create an Edn::Symbol
        let name = SYMBOLS.with(|symbols| {
            let symbols = symbols.borrow();
            symbols.get(id as usize).cloned().unwrap_or_default()
        });
        WitSexp::new(SexpResource { inner: Edn::Symbol(Symbol { namespace: None, name }) })
    }

    fn make_keyword(id: u32) -> WitSexp {
        // Look up the keyword name and create an Edn::Keyword
        let name = KEYWORDS.with(|keywords| {
            let keywords = keywords.borrow();
            keywords.get(id as usize).cloned().unwrap_or_default()
        });
        WitSexp::new(SexpResource { inner: Edn::Keyword(Keyword { namespace: None, name }) })
    }

    fn make_string(s: String) -> WitSexp {
        WitSexp::new(SexpResource { inner: Edn::String(s) })
    }

    fn make_char(c: char) -> WitSexp {
        WitSexp::new(SexpResource { inner: Edn::Char(c) })
    }

    fn make_list(items: Vec<WitSexp>) -> WitSexp {
        let internal_items: Vec<Edn> = items
            .into_iter()
            .map(|s| {
                // Extract the inner edn from the resource
                let resource: SexpResource = s.into_inner();
                resource.inner
            })
            .collect();
        WitSexp::new(SexpResource { inner: Edn::List(internal_items) })
    }

    fn make_vector(items: Vec<WitSexp>) -> WitSexp {
        let internal_items: Vec<Edn> = items
            .into_iter()
            .map(|s| {
                let resource: SexpResource = s.into_inner();
                resource.inner
            })
            .collect();
        WitSexp::new(SexpResource { inner: Edn::Vector(internal_items) })
    }
}

fn internal_error_to_wit(e: InternalParseError) -> ParseError {
    ParseError {
        message: e.message,
        span: Span {
            start: e.span.start as u32,
            end: e.span.end as u32,
        },
        expected: e.expected,
    }
}

fn wit_to_internal_bigint(b: Bigint) -> BigInt {
    let magnitude = BigInt::from_bytes_le(num_bigint::Sign::Plus, &b.bytes);
    if b.negative {
        -magnitude
    } else {
        magnitude
    }
}

fn wit_to_internal_number(n: WitNumber) -> InternalNumber {
    use num_rational::BigRational;
    match n {
        WitNumber::Integer(b) => InternalNumber::Integer(wit_to_internal_bigint(b)),
        WitNumber::Ratio(r) => {
            let numer = wit_to_internal_bigint(r.numer);
            let denom = wit_to_internal_bigint(r.denom);
            InternalNumber::Ratio(BigRational::new(numer, denom))
        }
        WitNumber::Float(f) => InternalNumber::Float(f),
    }
}

export!(Component);
