//! Numeric types for Suss
//!
//! Suss supports a full numeric tower: integers (arbitrary precision),
//! ratios (exact fractions), and floats (IEEE 754 f64).

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{One, Zero};
use std::fmt;
use std::str::FromStr;

/// A Suss number
#[derive(Debug, Clone, PartialEq)]
pub enum Number {
    /// Arbitrary precision integer
    Integer(BigInt),
    /// Exact ratio of two integers
    Ratio(BigRational),
    /// IEEE 754 double precision float
    Float(f64),
}

impl Number {
    /// Parse an integer from a string
    pub fn parse_integer(s: &str) -> Option<Self> {
        // Handle radix prefixes
        let (radix, digits) = if let Some(rest) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
            (16, rest)
        } else if let Some(rest) = s.strip_prefix("0o").or_else(|| s.strip_prefix("0O")) {
            (8, rest)
        } else if let Some(rest) = s.strip_prefix("0b").or_else(|| s.strip_prefix("0B")) {
            (2, rest)
        } else if s.contains('r') || s.contains('R') {
            // Clojure-style radix: 2r1010, 16rFF
            let parts: Vec<&str> = s.splitn(2, |c| c == 'r' || c == 'R').collect();
            if parts.len() == 2 {
                let radix: u32 = parts[0].parse().ok()?;
                if radix < 2 || radix > 36 {
                    return None;
                }
                (radix, parts[1])
            } else {
                return None;
            }
        } else {
            (10, s)
        };

        BigInt::parse_bytes(digits.as_bytes(), radix).map(Number::Integer)
    }

    /// Parse a ratio from a string (e.g., "1/3")
    pub fn parse_ratio(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.splitn(2, '/').collect();
        if parts.len() != 2 {
            return None;
        }
        let numer = BigInt::from_str(parts[0]).ok()?;
        let denom = BigInt::from_str(parts[1]).ok()?;
        if denom.is_zero() {
            return None;
        }
        Some(Number::Ratio(BigRational::new(numer, denom)))
    }

    /// Parse a float from a string
    pub fn parse_float(s: &str) -> Option<Self> {
        // Handle special values
        match s {
            "##Inf" => return Some(Number::Float(f64::INFINITY)),
            "##-Inf" => return Some(Number::Float(f64::NEG_INFINITY)),
            "##NaN" => return Some(Number::Float(f64::NAN)),
            _ => {}
        }
        f64::from_str(s).ok().map(Number::Float)
    }

    /// Check if this number is zero
    pub fn is_zero(&self) -> bool {
        match self {
            Number::Integer(n) => n.is_zero(),
            Number::Ratio(r) => r.is_zero(),
            Number::Float(f) => *f == 0.0,
        }
    }

    /// Check if this number is one
    pub fn is_one(&self) -> bool {
        match self {
            Number::Integer(n) => n.is_one(),
            Number::Ratio(r) => r.is_one(),
            Number::Float(f) => *f == 1.0,
        }
    }
}

impl fmt::Display for Number {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Number::Integer(n) => write!(f, "{}", n),
            Number::Ratio(r) => write!(f, "{}/{}", r.numer(), r.denom()),
            Number::Float(n) => {
                if n.is_infinite() {
                    if n.is_sign_positive() {
                        write!(f, "##Inf")
                    } else {
                        write!(f, "##-Inf")
                    }
                } else if n.is_nan() {
                    write!(f, "##NaN")
                } else {
                    write!(f, "{}", n)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_integer() {
        assert_eq!(
            Number::parse_integer("42"),
            Some(Number::Integer(BigInt::from(42)))
        );
        assert_eq!(
            Number::parse_integer("-17"),
            Some(Number::Integer(BigInt::from(-17)))
        );
        assert_eq!(
            Number::parse_integer("0xFF"),
            Some(Number::Integer(BigInt::from(255)))
        );
        assert_eq!(
            Number::parse_integer("2r1010"),
            Some(Number::Integer(BigInt::from(10)))
        );
    }

    #[test]
    fn test_parse_ratio() {
        let r = Number::parse_ratio("1/3").unwrap();
        if let Number::Ratio(r) = r {
            assert_eq!(r.numer(), &BigInt::from(1));
            assert_eq!(r.denom(), &BigInt::from(3));
        } else {
            panic!("Expected ratio");
        }
    }

    #[test]
    fn test_parse_float() {
        assert_eq!(Number::parse_float("3.14"), Some(Number::Float(3.14)));
        assert!(matches!(Number::parse_float("##Inf"), Some(Number::Float(f)) if f.is_infinite()));
    }
}
