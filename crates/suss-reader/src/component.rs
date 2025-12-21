//! WASM Component Model bindings for suss-reader
//!
//! This module implements the reader WIT interface using resource handles.

use std::cell::RefCell;

wit_bindgen::generate!({
    world: "reader-world",
    path: "../../wit",
});

use crate::{parse_and_intern, parse_all_and_intern, ParseError as InternalParseError, ParserState};
use suss_core::{print_sexp, Number as InternalNumber, Sexp as InternalSexp, KeywordId, SymbolId};
use num_bigint::BigInt;

// Types from the exported types interface
use exports::suss::lang::types::{GuestSexp, ParseError, Span, Sexp as WitSexp, Number as WitNumber, Bigint};
// Reader guest trait
use exports::suss::lang::reader::Guest as ReaderGuest;
use exports::suss::lang::types::Guest as TypesGuest;

// Global state for the reader component
thread_local! {
    static STATE: RefCell<ParserState> = RefCell::new(ParserState::new("suss"));
}

/// Resource wrapper for internal Sexp
/// This is an opaque handle that owns the parsed s-expression
pub struct SexpResource {
    inner: InternalSexp,
}

impl GuestSexp for SexpResource {
    fn to_string(&self) -> String {
        STATE.with(|state| {
            let state = state.borrow();
            print_sexp(&self.inner, &state.interner)
        })
    }

    fn is_nil(&self) -> bool {
        matches!(self.inner, InternalSexp::Nil)
    }

    fn is_truthy(&self) -> bool {
        !matches!(self.inner, InternalSexp::Nil | InternalSexp::Bool(false))
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

            match parse_and_intern(&input, &mut state) {
                Ok(sexp) => Ok(WitSexp::new(SexpResource { inner: sexp })),
                Err(e) => Err(internal_error_to_wit(e)),
            }
        })
    }

    fn read_all(input: String, platform: String) -> Result<Vec<WitSexp>, ParseError> {
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.platform = platform;

            match parse_all_and_intern(&input, &mut state) {
                Ok(sexps) => Ok(sexps
                    .into_iter()
                    .map(|sexp| WitSexp::new(SexpResource { inner: sexp }))
                    .collect()),
                Err(e) => Err(internal_error_to_wit(e)),
            }
        })
    }

    fn intern_symbol(name: String) -> u32 {
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.interner.intern_symbol(&name).0
        })
    }

    fn symbol_name(id: u32) -> String {
        STATE.with(|state| {
            let state = state.borrow();
            state.interner.symbol_name(SymbolId(id)).to_string()
        })
    }

    fn intern_keyword(name: String) -> u32 {
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.interner.intern_keyword(&name).0
        })
    }

    fn keyword_name(id: u32) -> String {
        STATE.with(|state| {
            let state = state.borrow();
            state.interner.keyword_name(KeywordId(id)).to_string()
        })
    }

    // === Factory functions ===

    fn make_nil() -> WitSexp {
        WitSexp::new(SexpResource { inner: InternalSexp::Nil })
    }

    fn make_bool(b: bool) -> WitSexp {
        WitSexp::new(SexpResource { inner: InternalSexp::Bool(b) })
    }

    fn make_number(n: WitNumber) -> WitSexp {
        WitSexp::new(SexpResource { inner: InternalSexp::Number(wit_to_internal_number(n)) })
    }

    fn make_symbol(id: u32) -> WitSexp {
        WitSexp::new(SexpResource { inner: InternalSexp::Symbol(SymbolId(id)) })
    }

    fn make_keyword(id: u32) -> WitSexp {
        WitSexp::new(SexpResource { inner: InternalSexp::Keyword(KeywordId(id)) })
    }

    fn make_string(s: String) -> WitSexp {
        WitSexp::new(SexpResource { inner: InternalSexp::String(s) })
    }

    fn make_char(c: char) -> WitSexp {
        WitSexp::new(SexpResource { inner: InternalSexp::Char(c) })
    }

    fn make_list(items: Vec<WitSexp>) -> WitSexp {
        let internal_items: Vec<InternalSexp> = items
            .into_iter()
            .map(|s| {
                // Extract the inner sexp from the resource
                // This consumes the resource and takes ownership of the inner sexp
                let resource: SexpResource = s.into_inner();
                resource.inner
            })
            .collect();
        WitSexp::new(SexpResource { inner: InternalSexp::List(internal_items) })
    }

    fn make_vector(items: Vec<WitSexp>) -> WitSexp {
        let internal_items: Vec<InternalSexp> = items
            .into_iter()
            .map(|s| {
                let resource: SexpResource = s.into_inner();
                resource.inner
            })
            .collect();
        WitSexp::new(SexpResource { inner: InternalSexp::Vector(internal_items) })
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
