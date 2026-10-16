//! Borrowed grammar of one raw key. Hashes choose buckets; stored bytes prove identity.
use super::{Node, append, reserve, size};
use suss_reader::forms::{Form, Kind};

pub(super) struct View<'a> {
    pub(super) form: &'a Form,
    pub(super) node: &'a Node,
    pub(super) nodes: &'a [Node],
    pub(super) groups: Option<&'a [(usize, usize)]>,
}
enum Field<'a> {
    Size(usize),
    Byte(u8),
    Bytes(&'a [u8]),
    Bits(u64),
    Units(&'a [u16]),
}
impl View<'_> {
    // This is the unchanged canonical byte grammar, traversed without creating
    // a token arena. A comparator stops immediately on its first mismatch.
    fn visit<E>(
        &self,
        charge: &mut impl FnMut(u64) -> Result<(), E>,
        mut field: impl FnMut(Field<'_>, &mut dyn FnMut(u64) -> Result<(), E>) -> Result<bool, E>,
    ) -> Result<bool, E> {
        charge(4)?; // form/node reads, metadata/body partition and dispatch
        macro_rules! emit {
            ($value:expr) => {{
                charge(1)?; // field dispatch
                if !field($value, charge)? {
                    return Ok(false);
                }
            }};
        }
        emit!(Field::Size(self.node.metadata_count));
        for child in &self.node.children[..self.node.metadata_count] {
            charge(3)?; // loop, child index and canonical key read
            emit!(Field::Size(self.nodes[*child].key.0));
        }
        let tag = match &self.form.kind {
            Kind::Nil => 0,
            Kind::Bool(_) => 1,
            Kind::Number(_) => 2,
            Kind::String(_) => 3,
            Kind::Symbol(_) => 4,
            Kind::Keyword(_) => 5,
            Kind::List(_) => 6,
            Kind::Vector(_) => 7,
            Kind::Set(_) => 8,
            Kind::Map(_) => 9,
            Kind::Conditional(_) => 10,
            Kind::Discard(_) => 11,
            Kind::Prefix { .. } => 12,
            Kind::SplicingConditional(_) => 13,
        };
        emit!(Field::Byte(tag));
        match &self.form.kind {
            Kind::Bool(value) => {
                emit!(Field::Byte(u8::from(*value)));
            }
            Kind::Number(value) => {
                emit!(Field::Bits(value.to_bits()));
            }
            Kind::String(value) => {
                emit!(Field::Size(value.len()));
                emit!(Field::Units(value));
            }
            Kind::Symbol(symbol) => {
                emit!(Field::Byte(u8::from(symbol.namespace.is_some())));
                if let Some(ns) = &symbol.namespace {
                    emit!(Field::Size(ns.len()));
                    emit!(Field::Bytes(ns.as_bytes()));
                }
                emit!(Field::Size(symbol.name.len()));
                emit!(Field::Bytes(symbol.name.as_bytes()));
            }
            Kind::Keyword(keyword) => {
                emit!(Field::Byte(u8::from(keyword.namespace.is_some())));
                if let Some(ns) = &keyword.namespace {
                    emit!(Field::Size(ns.len()));
                    emit!(Field::Bytes(ns.as_bytes()));
                }
                emit!(Field::Size(keyword.name.len()));
                emit!(Field::Bytes(keyword.name.as_bytes()));
            }
            Kind::List(v)
            | Kind::Vector(v)
            | Kind::Set(v)
            | Kind::Map(v)
            | Kind::Conditional(v)
            | Kind::SplicingConditional(v) => {
                emit!(Field::Size(v.len()));
            }
            _ => {}
        }
        if let Some(groups) = self.groups {
            for (first, second) in groups {
                charge(1)?;
                emit!(Field::Size(*first));
                if matches!(self.form.kind, Kind::Map(_)) {
                    emit!(Field::Size(*second));
                }
            }
        } else {
            for child in &self.node.children[self.node.metadata_count..] {
                charge(3)?; // loop, child index and canonical key read
                emit!(Field::Size(self.nodes[*child].key.0));
            }
        }
        Ok(true)
    }
    pub(super) fn hash<E>(&self, charge: &mut impl FnMut(u64) -> Result<(), E>) -> Result<u64, E> {
        charge(1)?;
        let mut hash = 0xcbf29ce484222325u64;
        self.visit(charge, |field, charge| {
            // Typed words avoid LE/ULEB encoding on hits. The canonical grammar
            // uniquely determines these field boundaries. Equal stored bytes
            // therefore produce equal word streams, including empty payloads.
            match field {
                Field::Size(v) => {
                    charge(4)?;
                    hash = (hash ^ v as u64).wrapping_mul(0x100000001b3);
                }
                Field::Byte(v) => {
                    charge(4)?;
                    hash = (hash ^ u64::from(v)).wrapping_mul(0x100000001b3);
                }
                Field::Bits(v) => {
                    charge(4)?;
                    hash = (hash ^ v).wrapping_mul(0x100000001b3);
                }
                Field::Bytes(v) => {
                    for chunk in v.chunks(256) {
                        charge(2 + (chunk.len() as u64) * 3)?;
                        for byte in chunk {
                            hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
                        }
                    }
                }
                Field::Units(v) => {
                    for chunk in v.chunks(128) {
                        charge(2 + (chunk.len() as u64) * 5)?;
                        for unit in chunk {
                            hash = (hash ^ u64::from(*unit)).wrapping_mul(0x100000001b3);
                        }
                    }
                }
            }
            Ok(true)
        })?;
        // Finalize the whole word stream before RawHeads masks its low bits.
        // Prepay all three shifts/xors, two wrapping multiplies and fixed
        // assignment/return control. Existing stream tariffs are unchanged.
        charge(10)?;
        hash = (hash ^ (hash >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        hash = (hash ^ (hash >> 27)).wrapping_mul(0x94d049bb133111eb);
        hash ^= hash >> 31;
        Ok(hash)
    }
    pub(super) fn encode<E>(
        &self,
        charge: &mut impl FnMut(u64) -> Result<(), E>,
    ) -> Result<Vec<u8>, E> {
        let mut out = Vec::new();
        self.visit(charge, |field, charge| {
            let mut charge = |n| charge(n);
            match field {
                Field::Size(v) => size(&mut out, v, &mut charge)?,
                Field::Byte(v) => append(&mut out, &[v], &mut charge)?,
                Field::Bits(v) => append(&mut out, &v.to_le_bytes(), &mut charge)?,
                Field::Bytes(v) => append(&mut out, v, &mut charge)?,
                Field::Units(v) => {
                    reserve(&mut out, v.len().saturating_mul(2), &mut charge)?;
                    for unit in v {
                        append(&mut out, &unit.to_le_bytes(), &mut charge)?;
                    }
                }
            }
            Ok(true)
        })?;
        Ok(out)
    }
    pub(super) fn matches<E>(
        &self,
        bytes: &[u8],
        charge: &mut impl FnMut(u64) -> Result<(), E>,
    ) -> Result<bool, E> {
        let mut cursor = 0usize;
        let equal = self.visit(charge, |field, charge| {
            match field {
                Field::Size(expected) => {
                    // Direct bounded decoding avoids a variable loop for the
                    // overwhelmingly common one/two-byte stored integers.
                    // These checks read canonical bytes; no expected encoding
                    // or temporary byte storage is constructed on a hit.
                    if expected < 128 {
                        charge(5)?; // bound, stored read, cast/equality, cursor
                        let actual = bytes.get(cursor).copied();
                        cursor += usize::from(actual.is_some());
                        return Ok(actual == Some(expected as u8));
                    }
                    if expected < 16384 {
                        charge(11)?; // two reads, continuation/canonical bounds,
                        // mask, shift, merge, equality and cursor
                        let Some(pair) = bytes.get(cursor..).and_then(|v| v.get(..2)) else {
                            return Ok(false);
                        };
                        cursor += 2;
                        return Ok(pair[0] >= 128
                            && pair[1] > 0
                            && pair[1] < 128
                            && (usize::from(pair[0] & 127) | (usize::from(pair[1]) << 7))
                                == expected);
                    }
                    let mut value = 0u64;
                    let mut shift = 0u32;
                    loop {
                        // Read/cursor, mask, shift/overflow bounds, merge,
                        // continuation and loop advance; no unbilled decoding.
                        charge(8)?;
                        let Some(byte) = bytes.get(cursor).copied() else {
                            return Ok(false);
                        };
                        cursor += 1;
                        let payload = u64::from(byte & 127);
                        if shift >= 64 || payload > (u64::MAX >> shift) {
                            return Ok(false);
                        }
                        value |= payload << shift;
                        if byte & 128 == 0 {
                            charge(2)?; // canonical termination and word equality
                            return Ok((shift == 0 || payload != 0) && value == expected as u64);
                        }
                        shift += 7;
                    }
                }
                Field::Byte(expected) => {
                    charge(4)?;
                    let actual = bytes.get(cursor).copied();
                    cursor += usize::from(actual.is_some());
                    Ok(actual == Some(expected))
                }
                Field::Bits(expected) => {
                    charge(12)?; // bounded 8-byte read/copy, LE word and equality
                    let Some(actual) = bytes.get(cursor..).and_then(|v| v.get(..8)) else {
                        return Ok(false);
                    };
                    cursor += 8;
                    Ok(u64::from_le_bytes(actual.try_into().unwrap()) == expected)
                }
                Field::Bytes(expected) => {
                    for byte in expected {
                        charge(3)?; // both reads, comparison/cursor
                        if bytes.get(cursor) != Some(byte) {
                            return Ok(false);
                        }
                        cursor += 1;
                    }
                    charge(0)?;
                    Ok(true)
                }
                Field::Units(expected) => {
                    for unit in expected {
                        charge(7)?; // bounded two-byte read, LE decode, compare/cursor
                        let Some(actual) = bytes.get(cursor..).and_then(|v| v.get(..2)) else {
                            return Ok(false);
                        };
                        if u16::from_le_bytes([actual[0], actual[1]]) != *unit {
                            return Ok(false);
                        }
                        cursor += 2;
                    }
                    charge(0)?;
                    Ok(true)
                }
            }
        })?;
        charge(1)?;
        Ok(equal && cursor == bytes.len())
    }
}
