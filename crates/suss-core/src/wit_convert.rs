//! Type conversions between internal Rust types and WIT-generated types
//!
//! This module provides bidirectional conversions for the Component Model.

#[cfg(feature = "component")]
use crate::{Interner, Number, Sexp, SymbolId, KeywordId};

#[cfg(feature = "component")]
use num_bigint::BigInt;

/// Convert a BigInt to WIT bigint representation (bytes + sign)
#[cfg(feature = "component")]
pub fn bigint_to_wit(n: &BigInt) -> (Vec<u8>, bool) {
    use num_traits::Signed;
    let negative = n.is_negative();
    let abs = if negative { -n } else { n.clone() };
    let bytes = abs.to_bytes_le().1;
    (bytes, negative)
}

/// Convert WIT bigint representation to BigInt
#[cfg(feature = "component")]
pub fn wit_to_bigint(bytes: Vec<u8>, negative: bool) -> BigInt {
    use num_bigint::Sign;
    let sign = if negative { Sign::Minus } else { Sign::Plus };
    BigInt::from_bytes_le(sign, &bytes)
}

/// Trait for converting to WIT types
#[cfg(feature = "component")]
pub trait ToWit<W> {
    fn to_wit(&self, interner: &Interner) -> W;
}

/// Trait for converting from WIT types
#[cfg(feature = "component")]
pub trait FromWit<W> {
    fn from_wit(wit: W, interner: &mut Interner) -> Self;
}

// Note: The actual WIT type implementations will be in the crates that
// use wit-bindgen, since the generated types are local to each crate.
// This module provides the helper functions and traits that those
// implementations will use.

/// Module with conversion helpers for use by component implementations
#[cfg(feature = "component")]
pub mod helpers {
    use super::*;

    /// Convert internal Number to components for WIT
    pub fn number_to_parts(num: &Number) -> NumberParts {
        match num {
            Number::Integer(n) => {
                let (bytes, negative) = bigint_to_wit(n);
                NumberParts::Integer { bytes, negative }
            }
            Number::Ratio(r) => {
                let (numer_bytes, numer_neg) = bigint_to_wit(r.numer());
                let (denom_bytes, denom_neg) = bigint_to_wit(r.denom());
                NumberParts::Ratio {
                    numer_bytes,
                    numer_negative: numer_neg,
                    denom_bytes,
                    denom_negative: denom_neg,
                }
            }
            Number::Float(f) => NumberParts::Float(*f),
        }
    }

    /// Convert WIT number parts back to internal Number
    pub fn parts_to_number(parts: NumberParts) -> Number {
        match parts {
            NumberParts::Integer { bytes, negative } => {
                Number::Integer(wit_to_bigint(bytes, negative))
            }
            NumberParts::Ratio {
                numer_bytes,
                numer_negative,
                denom_bytes,
                denom_negative,
            } => {
                let numer = wit_to_bigint(numer_bytes, numer_negative);
                let denom = wit_to_bigint(denom_bytes, denom_negative);
                Number::Ratio(num_rational::Ratio::new(numer, denom))
            }
            NumberParts::Float(f) => Number::Float(f),
        }
    }

    /// Intermediate representation for Number conversion
    pub enum NumberParts {
        Integer { bytes: Vec<u8>, negative: bool },
        Ratio {
            numer_bytes: Vec<u8>,
            numer_negative: bool,
            denom_bytes: Vec<u8>,
            denom_negative: bool,
        },
        Float(f64),
    }

    /// Convert internal Sexp to a form suitable for WIT conversion
    /// Returns components that can be used to build WIT sexp
    pub fn sexp_to_parts(sexp: &Sexp, interner: &Interner) -> SexpParts {
        match sexp {
            Sexp::Nil => SexpParts::Nil,
            Sexp::Bool(b) => SexpParts::Boolean(*b),
            Sexp::Symbol(id) => SexpParts::Symbol(id.0),
            Sexp::Keyword(id) => SexpParts::Keyword(id.0),
            Sexp::Number(n) => SexpParts::Number(number_to_parts(n)),
            Sexp::Char(c) => SexpParts::Character(*c),
            Sexp::String(s) => SexpParts::String(s.clone()),
            Sexp::List(items) => {
                SexpParts::List(items.iter().map(|s| sexp_to_parts(s, interner)).collect())
            }
            Sexp::Vector(items) => {
                SexpParts::Vector(items.iter().map(|s| sexp_to_parts(s, interner)).collect())
            }
            Sexp::Set(items) => {
                SexpParts::Set(items.iter().map(|s| sexp_to_parts(s, interner)).collect())
            }
            Sexp::Map(pairs) => SexpParts::Map(
                pairs
                    .iter()
                    .map(|(k, v)| (sexp_to_parts(k, interner), sexp_to_parts(v, interner)))
                    .collect(),
            ),
            Sexp::ReaderConditional(clauses) => SexpParts::ReaderConditional(
                clauses
                    .iter()
                    .map(|(kw, expr)| (kw.0, sexp_to_parts(expr, interner)))
                    .collect(),
            ),
            // Primitives and Functions cannot cross component boundaries
            Sexp::Primitive(name) => {
                SexpParts::String(format!("#<primitive:{}>", name))
            }
            Sexp::Function { .. } => {
                SexpParts::String("#<function>".to_string())
            }
        }
    }

    /// Convert WIT sexp parts back to internal Sexp
    pub fn parts_to_sexp(parts: SexpParts, interner: &mut Interner) -> Sexp {
        match parts {
            SexpParts::Nil => Sexp::Nil,
            SexpParts::Boolean(b) => Sexp::Bool(b),
            SexpParts::Symbol(id) => Sexp::Symbol(SymbolId(id)),
            SexpParts::Keyword(id) => Sexp::Keyword(KeywordId(id)),
            SexpParts::Number(n) => Sexp::Number(parts_to_number(n)),
            SexpParts::Character(c) => Sexp::Char(c),
            SexpParts::String(s) => Sexp::String(s),
            SexpParts::List(items) => {
                Sexp::List(items.into_iter().map(|p| parts_to_sexp(p, interner)).collect())
            }
            SexpParts::Vector(items) => {
                Sexp::Vector(items.into_iter().map(|p| parts_to_sexp(p, interner)).collect())
            }
            SexpParts::Set(items) => {
                Sexp::Set(items.into_iter().map(|p| parts_to_sexp(p, interner)).collect())
            }
            SexpParts::Map(pairs) => Sexp::Map(
                pairs
                    .into_iter()
                    .map(|(k, v)| (parts_to_sexp(k, interner), parts_to_sexp(v, interner)))
                    .collect(),
            ),
            SexpParts::ReaderConditional(clauses) => Sexp::ReaderConditional(
                clauses
                    .into_iter()
                    .map(|(kw_id, expr)| (KeywordId(kw_id), parts_to_sexp(expr, interner)))
                    .collect(),
            ),
        }
    }

    /// Intermediate representation for Sexp conversion
    pub enum SexpParts {
        Nil,
        Boolean(bool),
        Symbol(u32),
        Keyword(u32),
        Number(NumberParts),
        Character(char),
        String(String),
        List(Vec<SexpParts>),
        Vector(Vec<SexpParts>),
        Set(Vec<SexpParts>),
        Map(Vec<(SexpParts, SexpParts)>),
        ReaderConditional(Vec<(u32, SexpParts)>),
    }
}
