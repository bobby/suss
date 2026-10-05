//! Compare lossless host observations to the unchanged reviewed corpus.
//! Never invoke guest equality or convert observations through a printer.
use super::portable_decode::Observation;
use suss_core::{Edn, Number};

pub fn matches(expected: &Edn, actual: &Observation) -> bool {
    match (expected, actual) {
        (Edn::Nil, Observation::Nil) => true,
        (Edn::Bool(a), Observation::Bool(b)) => a == b,
        (Edn::Number(Number::Float(a)), Observation::Number(bits)) => {
            let expected = a.to_bits();
            expected == *bits || (canonical_nan(expected) && canonical_nan(*bits))
        }
        (Edn::Number(Number::Integer(integer)), Observation::Number(bits)) => {
            let number = f64::from_bits(*bits);
            number.is_finite()
                && number.fract() == 0.0
                && *bits != (-0.0_f64).to_bits()
                && format!("{number:.0}") == integer.to_string()
        }
        (Edn::String(text), Observation::String(units)) => {
            text.encode_utf16().eq(units.iter().copied())
        }
        (Edn::Char(character), Observation::String(units)) => {
            let mut buffer = [0; 2];
            let expected = character.encode_utf16(&mut buffer);
            expected.len() == 1 && expected == units.as_slice()
        }
        (Edn::Keyword(key), Observation::Keyword(namespace, name)) => {
            identifier(&key.namespace, &key.name, namespace, name)
        }
        (Edn::Symbol(key), Observation::Symbol(namespace, name)) => {
            identifier(&key.namespace, &key.name, namespace, name)
        }
        (Edn::List(a) | Edn::Vector(a), Observation::List(b) | Observation::Vector(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| matches(a, b))
        }
        (Edn::Map(a), Observation::Map(b)) => unordered(a, b, |(ak, av), (bk, bv)| {
            matches(ak, bk) && matches(av, bv)
        }),
        (Edn::Set(a), Observation::Set(b)) => unordered(a, b, matches),
        _ => false,
    }
}

fn canonical_nan(bits: u64) -> bool {
    bits & 0x7fff_ffff_ffff_ffff == 0x7ff8_0000_0000_0000
}
fn identifier(
    expected_ns: &Option<String>,
    expected_name: &str,
    namespace: &Option<Vec<u16>>,
    name: &[u16],
) -> bool {
    let namespace_matches = match (expected_ns, namespace) {
        (None, None) => true,
        (Some(a), Some(b)) => a.encode_utf16().eq(b.iter().copied()),
        _ => false,
    };
    namespace_matches && expected_name.encode_utf16().eq(name.iter().copied())
}
fn unordered<A, B>(expected: &[A], actual: &[B], eq: impl Fn(&A, &B) -> bool) -> bool {
    if expected.len() != actual.len() {
        return false;
    }
    let mut used = vec![false; actual.len()];
    expected.iter().all(|expected| {
        let Some(index) = actual
            .iter()
            .enumerate()
            .position(|(i, value)| !used[i] && eq(expected, value))
        else {
            return false;
        };
        used[index] = true;
        true
    })
}
