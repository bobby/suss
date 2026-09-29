//! Independent host-side decoder for the *prototype* GC ABI.
//! No calls to Suss equality, sequence functions, or printer. Unknown layouts
//! fail closed. Replace alongside ABI v1, not by a wildcard for opaque values.
use std::collections::HashMap;
use suss_compile::Compiler;
use suss_core::{Edn, Keyword, Number, Symbol};
use wasmtime::{StorageType, Store, Val};

type Result<T> = std::result::Result<T, String>;

pub struct Decoder {
    types: HashMap<i32, String>,
    remaining: usize,
}

impl Decoder {
    pub fn new(compiler: &mut Compiler) -> Self {
        // Prototype uses contiguous dispatch tags from 5 (not the stale 256 comment).
        let mut next = 5;
        let types = compiler
            .ensure_core_loaded()
            .expect("core analysis")
            .deftypes
            .iter()
            .map(|ty| {
                let id = ty.reserved_type_id.map(|id| id as i32).unwrap_or_else(|| {
                    let id = next;
                    next += 1;
                    id
                });
                (id, ty.name.clone())
            })
            .collect();
        Self {
            types,
            remaining: 100_000,
        }
    }

    pub fn decode(&mut self, store: &mut Store<()>, val: &Val) -> Result<Edn> {
        self.value(store, val, 0)
    }

    fn step(&mut self, depth: usize) -> Result<()> {
        if depth > 128 || self.remaining == 0 {
            return Err("decoder traversal limit (cycle or oversized value)".into());
        }
        self.remaining -= 1;
        Ok(())
    }

    fn value(&mut self, store: &mut Store<()>, val: &Val, depth: usize) -> Result<Edn> {
        self.step(depth)?;
        let reference = match val {
            Val::AnyRef(None) => return Ok(Edn::Nil),
            Val::AnyRef(Some(reference)) => reference,
            _ => return Err(format!("expected boxed value, got {val:?}")),
        };
        if let Some(i31) = reference.as_i31(&*store).map_err(err)? {
            return match i31.get_i32() {
                0 => Ok(Edn::Nil),
                2 => Ok(Edn::Bool(false)),
                4 => Ok(Edn::Bool(true)),
                n if n & 1 != 0 => Ok(Edn::Number(Number::from_i64((n >> 1) as i64))),
                n => Err(format!("unknown i31 tag {n}")),
            };
        }
        if let Some(array) = reference.as_array(&*store).map_err(err)? {
            if !matches!(
                array.ty(&*store).map_err(err)?.element_type(),
                StorageType::I8
            ) {
                return Err("raw non-string array escaped as a language value".into());
            }
            let len = array.len(&*store).map_err(err)? as usize;
            if len > self.remaining {
                return Err("decoder string limit".into());
            }
            self.remaining -= len;
            let bytes = (0..len)
                .map(|i| {
                    array
                        .get(&mut *store, i as u32)
                        .map_err(err)
                        .and_then(|v| i32_value(&v).map(|v| v as u8))
                })
                .collect::<Result<Vec<_>>>()?;
            return String::from_utf8(bytes).map(Edn::String).map_err(err);
        }
        let fields = fields(store, val)?;
        let tag = i32_value(at(&fields, 0)?)?;
        match tag {
            0 => match at(&fields, 1)? {
                Val::I64(n) => Ok(Edn::Number(Number::from_i64(*n))),
                _ => Err("malformed Int64".into()),
            },
            1 => match at(&fields, 1)? {
                Val::F64(bits) => Ok(Edn::Number(Number::Float(f64::from_bits(*bits)))),
                _ => Err("malformed Float64".into()),
            },
            23 | 24 => {
                let ns = self.value(store, at(&fields, 2)?, depth + 1)?;
                let name = self.value(store, at(&fields, 3)?, depth + 1)?;
                let Edn::String(name) = name else {
                    return Err("invalid symbol name".into());
                };
                let qualified = match ns {
                    Edn::Nil => name,
                    Edn::String(ns) => format!("{ns}/{name}"),
                    _ => return Err("invalid symbol namespace".into()),
                };
                Ok(if tag == 23 {
                    Edn::Keyword(Keyword::parse(&qualified))
                } else {
                    Edn::Symbol(Symbol::parse(&qualified))
                })
            }
            _ => match self.types.get(&tag).map(String::as_str) {
                Some("PersistentVector") => {
                    let count = checked_count(at(&fields, 1)?)?;
                    let shift = checked_count(at(&fields, 2)?)?;
                    if shift > 30 {
                        return Err("invalid vector shift".into());
                    }
                    let tail_offset = if count < 32 {
                        0
                    } else {
                        ((count - 1) >> 5) << 5
                    };
                    let mut result = Vec::new();
                    for index in 0..count {
                        self.step(depth)?;
                        let mut array = if index >= tail_offset {
                            at(&fields, 4)?
                        } else {
                            at(&fields, 3)?
                        }
                        .clone();
                        if index < tail_offset {
                            let mut level = shift;
                            while level > 0 {
                                array = array_get(store, &array, (index >> level) & 31)?;
                                level = level.checked_sub(5).ok_or("invalid vector depth")?;
                            }
                        }
                        let item = array_get(store, &array, index & 31)?;
                        result.push(self.value(store, &item, depth + 1)?);
                    }
                    Ok(Edn::Vector(result))
                }
                Some("PersistentMap") | Some("PersistentSet") => {
                    let set = self.types[&tag] == "PersistentSet";
                    let count = checked_count(at(&fields, 1)?)? as usize;
                    let mut pairs = Vec::new();
                    self.hamt(store, at(&fields, 2)?, depth + 1, &mut pairs)?;
                    if pairs.len() != count {
                        return Err(format!("HAMT count mismatch: {count} vs {}", pairs.len()));
                    }
                    Ok(if set {
                        Edn::Set(pairs.into_iter().map(|(k, _)| k).collect())
                    } else {
                        Edn::Map(pairs)
                    })
                }
                Some("Cons") => {
                    let first = self.value(store, at(&fields, 1)?, depth + 1)?;
                    let rest = self.value(store, at(&fields, 2)?, depth + 1)?;
                    let mut items = vec![first];
                    match rest {
                        Edn::Nil => {}
                        Edn::List(rest) | Edn::Vector(rest) => items.extend(rest),
                        _ => return Err("non-sequential Cons tail".into()),
                    }
                    Ok(Edn::List(items))
                }
                Some("IndexedSeq") => {
                    let array = at(&fields, 1)?;
                    let start = checked_count(at(&fields, 2)?)?;
                    let len = array_len(store, array)?;
                    if start > len {
                        return Err("invalid IndexedSeq offset".into());
                    }
                    let mut items = Vec::new();
                    for i in start..len {
                        let v = array_get(store, array, i)?;
                        items.push(self.value(store, &v, depth + 1)?);
                    }
                    Ok(Edn::List(items))
                }
                Some("MapEntry") => Ok(Edn::Vector(vec![
                    self.value(store, at(&fields, 1)?, depth + 1)?,
                    self.value(store, at(&fields, 2)?, depth + 1)?,
                ])),
                // Realizing a thunk would run guest code; unsupported values
                // must be reported as decode errors, never claimed as passing.
                name => Err(format!("unsupported GC value tag {tag} ({name:?})")),
            },
        }
    }

    fn hamt(
        &mut self,
        store: &mut Store<()>,
        node: &Val,
        depth: usize,
        out: &mut Vec<(Edn, Edn)>,
    ) -> Result<()> {
        self.step(depth)?;
        if is_nil(store, node)? {
            return Ok(());
        }
        let fields = fields(store, node)?;
        let tag = i32_value(at(&fields, 0)?)?;
        match self.types.get(&tag).map(String::as_str) {
            Some("BitmapIndexedNode") | Some("HashCollisionNode") => {
                let collision = self.types[&tag] == "HashCollisionNode";
                let array = at(&fields, if collision { 3 } else { 2 })?;
                let len = array_len(store, array)?;
                if len % 2 != 0 {
                    return Err("odd HAMT entry array".into());
                }
                for i in (0..len).step_by(2) {
                    let key = array_get(store, array, i)?;
                    let val = array_get(store, array, i + 1)?;
                    if !collision && is_nil(store, &key)? {
                        self.hamt(store, &val, depth + 1, out)?;
                    } else {
                        out.push((
                            self.value(store, &key, depth + 1)?,
                            self.value(store, &val, depth + 1)?,
                        ));
                    }
                }
                Ok(())
            }
            Some("ArrayNode") => {
                let array = at(&fields, 2)?;
                for i in 0..array_len(store, array)? {
                    let child = array_get(store, array, i)?;
                    self.hamt(store, &child, depth + 1, out)?;
                }
                Ok(())
            }
            _ => Err(format!("unknown HAMT node {tag}")),
        }
    }
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn at(fields: &[Val], i: usize) -> Result<&Val> {
    fields.get(i).ok_or_else(|| format!("missing field {i}"))
}
fn i32_value(v: &Val) -> Result<i32> {
    v.i32()
        .ok_or_else(|| format!("expected i32 field, got {v:?}"))
}
fn checked_count(v: &Val) -> Result<u32> {
    u32::try_from(i32_value(v)?).map_err(err)
}
fn fields(store: &mut Store<()>, val: &Val) -> Result<Vec<Val>> {
    let Val::AnyRef(Some(reference)) = val else {
        return Err("expected struct reference".into());
    };
    let s = reference
        .as_struct(&*store)
        .map_err(err)?
        .ok_or("expected struct")?;
    Ok(s.fields(&mut *store).map_err(err)?.collect())
}
fn array_get(store: &mut Store<()>, val: &Val, i: u32) -> Result<Val> {
    let Val::AnyRef(Some(reference)) = val else {
        return Err("expected array reference".into());
    };
    let a = reference
        .as_array(&*store)
        .map_err(err)?
        .ok_or("expected array")?;
    a.get(store, i).map_err(err)
}
fn array_len(store: &Store<()>, val: &Val) -> Result<u32> {
    let Val::AnyRef(Some(reference)) = val else {
        return Err("expected array reference".into());
    };
    reference
        .as_array(store)
        .map_err(err)?
        .ok_or("expected array")?
        .len(store)
        .map_err(err)
}
fn is_nil(store: &Store<()>, val: &Val) -> Result<bool> {
    match val {
        Val::AnyRef(None) => Ok(true),
        Val::AnyRef(Some(r)) => Ok(r
            .as_i31(store)
            .map_err(err)?
            .is_some_and(|n| n.get_i32() == 0)),
        _ => Ok(false),
    }
}

/// Structural equality of decoded test values, independent of guest equality.
/// Seq/vector equality is intentional; maps and sets disregard iteration order.
pub fn values_match(a: &Edn, b: &Edn) -> bool {
    match (a, b) {
        (Edn::Number(a), Edn::Number(b)) => match (a, b) {
            (Number::Integer(a), Number::Integer(b)) => a == b,
            (Number::Float(a), Number::Float(b)) => a.to_bits() == b.to_bits(),
            (Number::Integer(i), Number::Float(f)) | (Number::Float(f), Number::Integer(i)) => {
                f.is_finite()
                    && f.fract() == 0.0
                    && f.to_bits() != (-0.0f64).to_bits()
                    && format!("{f:.0}") == i.to_string()
            }
            _ => a == b,
        },
        (Edn::Vector(a) | Edn::List(a), Edn::Vector(b) | Edn::List(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| values_match(a, b))
        }
        (Edn::Map(a), Edn::Map(b)) => unordered(a, b, |(ak, av), (bk, bv)| {
            values_match(ak, bk) && values_match(av, bv)
        }),
        (Edn::Set(a), Edn::Set(b)) => unordered(a, b, values_match),
        _ => a == b,
    }
}
fn unordered<T>(a: &[T], b: &[T], eq: impl Fn(&T, &T) -> bool) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut used = vec![false; b.len()];
    a.iter().all(
        |a| match b.iter().enumerate().position(|(i, b)| !used[i] && eq(a, b)) {
            Some(i) => {
                used[i] = true;
                true
            }
            None => false,
        },
    )
}
