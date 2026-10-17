//   Copyright (c) Rich Hickey. All rights reserved.
//   The use and distribution terms for this software are covered by the
//   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
//   which can be found in the file epl-v10.html at the root of this distribution.
//   By using this software in any fashion, you are agreeing to be bound by
//   the terms of this license.
//   You must not remove this notice, or any other, from this software.
// SPDX-License-Identifier: EPL-1.0

//! Quoted data and identifier constants for the shared runtime.
//! Constructor layout follows cljs/compiler.cljc emits-symbol/emits-keyword at
//! c4295f303100bbf5afac449242d30bca1126f1a1. Constant hash arithmetic follows
//! retained core.cljs m3-hash-unencoded-chars/hash-symbol/hash-keyword (EPL-1.0).
//! This is original Rust lowering; the source-backed core owns the actual types.
use super::*;
use std::borrow::Cow;

impl Analyzer<'_> {
    pub(super) fn identifier_literal(
        &mut self,
        form: &Form,
        namespace: Option<&str>,
        name: &str,
        keyword: bool,
    ) -> Result<Hir, Diagnostic> {
        if !form.metadata.is_empty() {
            return Err(fail(
                form.span.clone(),
                "Quoted runtime metadata needs persistent metadata maps, not yet implemented",
            ));
        }
        let symbol = suss_reader::Symbol {
            namespace: Some("suss.core".into()),
            name: if keyword { "Keyword" } else { "Symbol" }.into(),
        };
        let (kind, ty) = self.global_value(&symbol, form.span.clone())?;
        let constructor = Hir {
            source: None,
            span: form.span.clone(),
            metadata: vec![],
            ty,
            kind,
        };
        let ns = self.literal_form(
            form,
            namespace.map_or(Literal::Nil, |value| {
                Literal::String(value.encode_utf16().collect())
            }),
        );
        let short = self.literal_form(form, Literal::String(name.encode_utf16().collect()));
        let full = namespace.map_or_else(|| name.to_owned(), |ns| format!("{ns}/{name}"));
        let full = self.literal_form(form, Literal::String(full.encode_utf16().collect()));
        let hash = self.literal_form(
            form,
            Literal::Number(identifier_hash(namespace, name, keyword) as f64),
        );
        let mut arguments = vec![constructor, ns, short, full, hash];
        if !keyword {
            arguments.push(self.literal_form(form, Literal::Nil));
        }
        Ok(self.nominal(form, Nominal::Construct, arguments))
    }
    pub(super) fn quote_data(&mut self, form: &Form, depth: usize) -> Result<Hir, Diagnostic> {
        self.quote_data_impl(form, depth, false)
    }
    /// Native form transport uses the same lowering while retaining reader data
    /// that the pinned compiler elides from ordinary runtime quoted constants.
    pub(super) fn quote_form_data(&mut self, form: &Form) -> Result<Hir, Diagnostic> {
        self.quote_data_impl(form, 0, true)
    }
    fn quote_data_impl(
        &mut self,
        form: &Form,
        depth: usize,
        reader_data: bool,
    ) -> Result<Hir, Diagnostic> {
        self.expander.charge_expansion_work(1, form.span.clone())?;
        stacker::maybe_grow(256 * 1024, 4 * 1024 * 1024, || {
            self.quote_data_inner(form, depth, reader_data)
        })
    }
    fn quote_data_inner(
        &mut self,
        form: &Form,
        depth: usize,
        reader_data: bool,
    ) -> Result<Hir, Diagnostic> {
        if !form.metadata.is_empty() {
            if !matches!(
                form.kind,
                Kind::Symbol(_) | Kind::List(_) | Kind::Vector(_) | Kind::Map(_) | Kind::Set(_)
            ) {
                return Err(fail(
                    form.span.clone(),
                    "Metadata requires a symbol or collection",
                ));
            }
            let mut bare = form.clone();
            bare.metadata.clear();
            let value = self.quote_data_impl(&bare, depth, reader_data)?;
            return self.attach_data_metadata(form, value, depth, reader_data);
        }
        match &form.kind {
            Kind::Symbol(value) => {
                self.identifier_literal(form, value.namespace.as_deref(), &value.name, false)
            }
            Kind::Keyword(value) => {
                self.identifier_literal(form, value.namespace.as_deref(), &value.name, true)
            }
            // These are private literal operands of quotation, not ordinary
            // analyzed source expressions. Calling form() here would attach
            // the datum's source record and prevent the enclosing quote from
            // retaining its own genuine syntax and source inference facts.
            Kind::Nil => Ok(self.literal_form(form, Literal::Nil)),
            Kind::Bool(value) => Ok(self.literal_form(form, Literal::Bool(*value))),
            Kind::Number(value) => Ok(self.literal_form(form, Literal::Number(*value))),
            Kind::String(value) => Ok(self.literal_form(form, Literal::String(value.clone()))),
            Kind::Vector(items) => {
                let entries = items
                    .iter()
                    .map(|item| self.quote_data_impl(item, depth + 1, reader_data))
                    .collect::<Result<Vec<_>, _>>()?;
                self.vector_values(form, entries)
            }
            Kind::Map(items) => {
                if items.len() % 2 != 0 {
                    return Err(fail(
                        form.span.clone(),
                        "Map literal requires paired entries",
                    ));
                }
                let entries = items
                    .iter()
                    .map(|item| self.quote_data_impl(item, depth + 1, reader_data))
                    .collect::<Result<Vec<_>, _>>()?;
                self.map_values(form, items, entries)
            }
            Kind::Set(items) => {
                let entries = items
                    .iter()
                    .map(|item| self.quote_data_impl(item, depth + 1, reader_data))
                    .collect::<Result<Vec<_>, _>>()?;
                self.set_values(form, items, entries)
            }
            Kind::List(items) => {
                let symbol = suss_reader::Symbol {
                    namespace: Some("suss.core".into()),
                    name: "list".into(),
                };
                let (kind, ty) = self.global_value(&symbol, form.span.clone())?;
                let callee = Hir {
                    source: None,
                    span: form.span.clone(),
                    metadata: vec![],
                    ty,
                    kind,
                };
                let arguments = items
                    .iter()
                    .map(|value| self.quote_data_impl(value, depth + 1, reader_data))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Hir {
                    source: None,
                    span: form.span.clone(),
                    metadata: vec![],
                    ty: Type::Value,
                    kind: Expression::Call {
                        callee: Box::new(callee),
                        arguments,
                    },
                })
            }
            _ => Err(fail(
                form.span.clone(),
                "Unresolved reader dispatch or prefix data cannot be quoted",
            )),
        }
    }
    fn data_core_call(
        &mut self,
        form: &Form,
        name: &str,
        arguments: Vec<Hir>,
    ) -> Result<Hir, Diagnostic> {
        let symbol = suss_reader::Symbol {
            namespace: Some("suss.core".into()),
            name: name.into(),
        };
        let (kind, ty) = self.global_value(&symbol, form.span.clone())?;
        let callee = Hir {
            source: None,
            span: form.span.clone(),
            metadata: vec![],
            kind,
            ty,
        };
        Ok(Hir {
            source: None,
            span: form.span.clone(),
            metadata: vec![],
            ty: Type::Value,
            kind: Expression::Call {
                callee: Box::new(callee),
                arguments,
            },
        })
    }
    pub(super) fn attach_literal_metadata(
        &mut self,
        form: &Form,
        value: Hir,
    ) -> Result<Hir, Diagnostic> {
        // Ordinary vector/map/set metadata is an expression, analyzed in the
        // surrounding lexical environment (pinned analyze-wrap-meta). Reader
        // prefix merging happens before analysis: discarded values never run.
        let entries = merged_metadata_pairs_metered(form, false, &mut |work| {
            self.expander.charge_expansion_work(work, form.span.clone())
        })?;
        if entries.is_empty() {
            return Ok(value);
        }
        // Analyze the actual merged metadata map in its own source frame. It
        // must not overwrite the enclosing literal's original child records.
        let metadata_form = Form {
            span: form.span.clone(),
            metadata: vec![],
            kind: Kind::Map(entries),
        };
        let metadata = self.form(&metadata_form)?;
        let slot = self.source_nodes.last_mut().expect("source node fact slot");
        let inner = slot.take();
        *slot = Some(std::sync::Arc::new(SourceNode::WithMeta {
            expression: std::sync::Arc::new(value.clone()),
            metadata: std::sync::Arc::new(metadata.clone()),
            inner,
        }));
        self.data_core_call(form, "with-meta", vec![value, metadata])
    }
    pub(super) fn attach_constant_metadata(
        &mut self,
        form: &Form,
        value: Hir,
    ) -> Result<Hir, Diagnostic> {
        // Empty-list literals are constants, so their metadata remains data.
        self.attach_data_metadata(form, value, 0, false)
    }
    fn attach_data_metadata(
        &mut self,
        form: &Form,
        value: Hir,
        depth: usize,
        reader_data: bool,
    ) -> Result<Hir, Diagnostic> {
        if form.metadata.is_empty() {
            return Ok(value);
        }
        let empty = self.map_values(form, &[], vec![])?;
        let mut result = self.fresh_binding(form, empty)?;
        let mut bindings = vec![result.clone()];
        let mut retained = reader_data;
        // Reader-data merging precedes construction. Actual source -assoc
        // supplies equality of the resulting compiled runtime keys.
        let entries = merged_metadata_pairs_metered(form, reader_data, &mut |work| {
            self.expander.charge_expansion_work(work, form.span.clone())
        })?;
        for pair in entries.chunks_exact(2) {
            let key = self.quote_data_impl(&pair[0], depth + 2, reader_data)?;
            let item = self.quote_data_impl(&pair[1], depth + 2, reader_data)?;
            let previous = self.local(form, result.id);
            let next = self.data_core_call(form, "-assoc", vec![previous, key, item])?;
            result = self.fresh_binding(form, next)?;
            bindings.push(result.clone());
            retained = true;
        }
        if !retained {
            return Ok(value);
        }
        let result = Hir {
            source: None,
            span: form.span.clone(),
            metadata: vec![],
            ty: Type::Value,
            kind: Expression::Let {
                bindings,
                body: Box::new(self.local(form, result.id)),
            },
        };
        self.data_core_call(form, "with-meta", vec![value, result])
    }
}

// Pinned tools.reader/read-meta (reader.clj386) merges inner metadata before
// outer metadata. Do this before constructing or evaluating any value.
/// Normalize reader metadata as data, using the same bounded key comparison
/// and prefix precedence as compiled quote-form lowering.
pub fn reader_metadata_pairs(form: &Form) -> Result<Vec<Form>, Diagnostic> {
    reader_metadata_pairs_metered(form, &mut |_| Ok(()))
}
/// Analyzed metadata pairs, eliding reader-position keys the way the pinned
/// analyzer's elide-reader-meta path does before using form metadata as data.
pub fn reader_metadata_pairs_metered(
    form: &Form,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<Vec<Form>, Diagnostic> {
    merged_metadata_pairs_metered(form, true, meter)
}
pub fn analyzed_metadata_pairs(form: &Form) -> Result<Vec<Form>, Diagnostic> {
    merged_metadata_pairs(form, false)
}
fn merged_metadata_pairs(form: &Form, reader_data: bool) -> Result<Vec<Form>, Diagnostic> {
    merged_metadata_pairs_metered(form, reader_data, &mut |_| Ok(()))
}
fn merged_metadata_pairs_metered(
    form: &Form,
    reader_data: bool,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<Vec<Form>, Diagnostic> {
    let mut entries: Vec<(Form, Form, (usize, usize))> = Vec::new();
    for (prefix_index, prefix) in form.metadata.iter().enumerate().rev() {
        meter(1)?;
        let pairs = metadata_pairs_metered(prefix, meter)?;
        for (pair_index, pair) in pairs.chunks_exact(2).enumerate() {
            if !reader_data && irrelevant_metadata_key(&pair[0]) {
                continue;
            }
            let mut previous = None;
            for (index, (key, _, _)) in entries.iter().enumerate() {
                if reader_key_equal_metered(key, &pair[0], 0, meter)? {
                    previous = Some(index);
                    break;
                }
            }
            let order = (prefix_index, pair_index);
            if let Some(index) = previous {
                // Pinned tools.reader merges inner first. Association keeps
                // the existing key object and replaces only its value.
                entries[index].1 = super::super::clone_form_metered(&pair[1], meter)?;
                entries[index].2 = order;
            } else {
                entries.push((
                    super::super::clone_form_metered(&pair[0], meter)?,
                    super::super::clone_form_metered(&pair[1], meter)?,
                    order,
                ));
            }
        }
    }
    // Use the accepted textual order for surviving metadata entries, even
    // when the pinned reader's hash iteration incidentally reorders them.
    meter(
        (entries.len() as u64)
            .saturating_mul(u64::from(usize::BITS - entries.len().leading_zeros())),
    )?;
    entries.sort_by_key(|(_, _, order)| *order);
    Ok(entries
        .into_iter()
        .flat_map(|(key, value, _)| [key, value])
        .collect())
}

// Original reader-data normalization. This comparison is deliberately about
// syntax before evaluation, not equality of evaluated metadata keys. Reader
// locations/metadata do not participate; lists/vectors share sequential data
// equality, and maps/sets compare without an iteration-order dependency.
#[cfg(test)]
fn reader_key_equal(
    a: &Form,
    b: &Form,
    depth: usize,
    remaining: &mut usize,
) -> Result<bool, Diagnostic> {
    reader_key_equal_metered(a, b, depth, &mut |work| {
        *remaining = remaining
            .checked_sub(usize::try_from(work).unwrap_or(usize::MAX))
            .ok_or_else(|| fail(a.span.clone(), "Metadata key computation fuel exhausted"))?;
        Ok(())
    })
}
fn reader_key_equal_metered(
    a: &Form,
    b: &Form,
    depth: usize,
    work: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<bool, Diagnostic> {
    let units = |form: &Form| match &form.kind {
        Kind::String(units) => units.len() as u64,
        Kind::Symbol(name) => {
            (name.name.len() + name.namespace.as_ref().map_or(0, String::len)) as u64
        }
        Kind::Keyword(name) => {
            (name.name.len() + name.namespace.as_ref().map_or(0, String::len)) as u64
        }
        _ => 0,
    };
    work(1u64.saturating_add(units(a)).saturating_add(units(b)))?;
    stacker::maybe_grow(256 * 1024, 4 * 1024 * 1024, || {
        reader_key_equal_inner(a, b, depth, work)
    })
}
fn reader_key_equal_inner(
    a: &Form,
    b: &Form,
    depth: usize,
    work: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<bool, Diagnostic> {
    Ok(match (&a.kind, &b.kind) {
        (Kind::Nil, Kind::Nil) => true,
        (Kind::Bool(a), Kind::Bool(b)) => a == b,
        (Kind::Number(a), Kind::Number(b)) => a == b,
        (Kind::String(a), Kind::String(b)) => a == b,
        (Kind::Symbol(a), Kind::Symbol(b)) => a == b,
        (Kind::Keyword(a), Kind::Keyword(b)) => a == b,
        (Kind::List(a) | Kind::Vector(a), Kind::List(b) | Kind::Vector(b)) => {
            if a.len() != b.len() {
                return Ok(false);
            }
            for (a, b) in a.iter().zip(b) {
                if !reader_key_equal_metered(a, b, depth + 1, work)? {
                    return Ok(false);
                }
            }
            true
        }
        (Kind::Set(a), Kind::Set(b)) => {
            let a = reader_set_members(a, depth + 1, work)?;
            let b = reader_set_members(b, depth + 1, work)?;
            if a.len() != b.len() {
                return Ok(false);
            }
            for a in a {
                let mut found = false;
                for b in &b {
                    if reader_key_equal_metered(a, b, depth + 1, work)? {
                        found = true;
                        break;
                    }
                }
                if !found {
                    return Ok(false);
                }
            }
            true
        }
        (Kind::Map(a), Kind::Map(b)) => {
            let a = reader_map_entries(a, depth + 1, work)?;
            let b = reader_map_entries(b, depth + 1, work)?;
            if a.len() != b.len() {
                return Ok(false);
            }
            for a in a {
                let mut found = false;
                for b in &b {
                    if reader_key_equal_metered(a.0, b.0, depth + 1, work)? {
                        if !reader_key_equal_metered(a.1, b.1, depth + 1, work)? {
                            return Ok(false);
                        }
                        found = true;
                        break;
                    }
                }
                if !found {
                    return Ok(false);
                }
            }
            true
        }
        _ => false,
    })
}
// Native Forms can contain repeated map/set entries even though the pinned
// textual reader rejects some such spellings. Compare their canonical data,
// matching source map construction's last-value and set uniqueness semantics.
fn reader_map_entries<'a>(
    items: &'a [Form],
    depth: usize,
    work: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<Vec<(&'a Form, &'a Form)>, Diagnostic> {
    if items.len() % 2 != 0 {
        return Err(fail(
            items.last().unwrap().span.clone(),
            "Metadata key map requires paired entries",
        ));
    }
    let mut entries: Vec<(&Form, &Form)> = Vec::new();
    for pair in items.chunks_exact(2) {
        let mut previous = None;
        for (index, entry) in entries.iter().enumerate() {
            if reader_key_equal_metered(entry.0, &pair[0], depth, work)? {
                previous = Some(index);
                break;
            }
        }
        if let Some(index) = previous {
            entries[index].1 = &pair[1];
        } else {
            entries.push((&pair[0], &pair[1]));
        }
    }
    Ok(entries)
}
fn reader_set_members<'a>(
    items: &'a [Form],
    depth: usize,
    work: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<Vec<&'a Form>, Diagnostic> {
    let mut entries: Vec<&Form> = Vec::new();
    for item in items {
        let mut present = false;
        for previous in &entries {
            if reader_key_equal_metered(previous, item, depth, work)? {
                present = true;
                break;
            }
        }
        if !present {
            entries.push(item);
        }
    }
    Ok(entries)
}

fn irrelevant_metadata_key(form: &Form) -> bool {
    matches!(&form.kind, Kind::Keyword(key) if
        (key.namespace.is_none() && matches!(key.name.as_str(), "file" | "line" | "column" | "end-column" | "end-line" | "source"))
        || (key.namespace.as_deref() == Some("cljs.analyzer") && key.name == "analyzed"))
}
fn metadata_pairs_metered<'a>(
    prefix: &'a Form,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<Cow<'a, [Form]>, Diagnostic> {
    Ok(match &prefix.kind {
        // This is a view of genuine reader metadata. The merge below owns
        // and meters each retained key/value clone; an intermediate deep copy
        // neither contributes source facts nor changes association semantics.
        Kind::Map(entries) if entries.len() % 2 == 0 => Cow::Borrowed(entries.as_slice()),
        Kind::Keyword(_) => Cow::Owned(vec![
            super::super::clone_form_metered(prefix, meter)?,
            Form {
                span: prefix.span.clone(),
                metadata: vec![],
                kind: Kind::Bool(true),
            },
        ]),
        Kind::Symbol(_) | Kind::String(_) => Cow::Owned(vec![
            Form {
                span: prefix.span.clone(),
                metadata: vec![],
                kind: Kind::Keyword(suss_reader::Keyword {
                    namespace: None,
                    name: "tag".into(),
                }),
            },
            super::super::clone_form_metered(prefix, meter)?,
        ]),
        _ => {
            return Err(fail(
                prefix.span.clone(),
                "Invalid metadata: expected a map, keyword, symbol or string",
            ));
        }
    })
}

// Keep the pinned literal hash independent of redefinitions of public hash cells.
// The prototype's xxHash constant helpers have a different contract and are unused.
/// The compiler's pinned identifier hash, also used by native form transport.
pub fn identifier_hash(namespace: Option<&str>, name: &str, keyword: bool) -> i32 {
    fn mix_word(word: u32) -> u32 {
        word.wrapping_mul(0xcc9e2d51)
            .rotate_left(15)
            .wrapping_mul(0x1b873593)
    }
    let units: Vec<_> = name.encode_utf16().collect();
    let mut hash = 0u32;
    let mut pairs = units.chunks_exact(2);
    for pair in &mut pairs {
        let word = u32::from(pair[0]) | (u32::from(pair[1]) << 16);
        hash = (hash ^ mix_word(word))
            .rotate_left(13)
            .wrapping_mul(5)
            .wrapping_add(0xe6546b64);
    }
    if let [unit] = pairs.remainder() {
        hash ^= mix_word(u32::from(*unit));
    }
    hash ^= (units.len() as u32).wrapping_mul(2);
    hash ^= hash >> 16;
    hash = hash.wrapping_mul(0x85ebca6b);
    hash ^= hash >> 13;
    hash = hash.wrapping_mul(0xc2b2ae35);
    hash ^= hash >> 16;
    let ns = namespace.map_or(0, |ns| {
        ns.encode_utf16().fold(0u32, |hash, unit| {
            hash.wrapping_mul(31).wrapping_add(u32::from(unit))
        })
    });
    let shifted = ((hash as i32) >> 2) as u32;
    let combined = hash
        ^ ns.wrapping_add(0x9e3779b9)
            .wrapping_add(hash << 6)
            .wrapping_add(shifted);
    if keyword {
        combined.wrapping_add(0x9e3779b9) as i32
    } else {
        combined as i32
    }
}

#[cfg(test)]
mod metadata_reader_tests {
    use super::*;

    #[test]
    fn map_metadata_view_borrows_exact_entries_without_copy_or_charge() {
        let prefix = suss_reader::forms::read_forms("{:tag [a {:nested [b c]}] :private true}").unwrap().remove(0);
        let Kind::Map(entries) = &prefix.kind else { unreachable!() };
        let view = metadata_pairs_metered(&prefix, &mut |_| panic!("borrowed view does no copy work")).unwrap();
        assert!(matches!(&view, Cow::Borrowed(_)));
        assert!(std::ptr::eq(view.as_ref(), entries.as_slice()));
        assert!(std::ptr::eq(&view[1], &entries[1]));
        for shorthand in [":private", "string", "\"custom\""] {
            let prefix = suss_reader::forms::read_forms(shorthand).unwrap().remove(0);
            let mut charged = 0;
            let pairs = metadata_pairs_metered(&prefix, &mut |work| { charged += work; Ok(()) }).unwrap();
            assert!(matches!(pairs, Cow::Owned(_)));
            assert_eq!(pairs.len(), 2);
            assert!(charged > 0, "shorthand's real syntax clone remains metered");
        }
        let invalid = Form { kind: Kind::Map(vec![entries[0].clone()]), span: 19..23, metadata: vec![] };
        let error = metadata_pairs_metered(&invalid, &mut |_| panic!("invalid map does not clone")).unwrap_err();
        assert_eq!(error.span, 19..23);
        assert_eq!(error.message, "Invalid metadata: expected a map, keyword, symbol or string");
    }

    #[test]
    fn borrowed_map_merge_preserves_nested_metadata_collision_key_order_and_retry() {
        let read = |text| suss_reader::forms::read_forms(text).unwrap().remove(0);
        let mut outer_key = read(":same");
        outer_key.span = 3..8;
        let mut inner_key = read(":same");
        inner_key.span = 70..75;
        inner_key.metadata.push(read(":original-key"));
        let mut outer_value = Form { kind: Kind::String(vec![65; 2048]), span: 10..2060, metadata: vec![read("{:leaf true}")] };
        for _ in 0..12 {
            outer_value = Form { kind: Kind::Vector(vec![outer_value]), span: 10..2060, metadata: vec![read(":nested")] };
        }
        let outer = Form { kind: Kind::Map(vec![outer_key, outer_value.clone(), read(":other"), read("17")]), span: 1..2100, metadata: vec![] };
        let inner = Form { kind: Kind::Map(vec![inner_key.clone(), read("{:discarded [1 2 3]}"), read(":line"), read("91")]), span: 69..100, metadata: vec![] };
        let mut form = read("value");
        form.metadata = vec![outer, inner];
        let original = form.clone();
        let mut calls = 0;
        let mut charged = 0;
        let output = reader_metadata_pairs_metered(&form, &mut |work| { calls += 1; charged += work; Ok(()) }).unwrap();
        assert!(charged > 2048, "actual retained payload clone remains charged");
        assert_eq!(output, vec![inner_key.clone(), outer_value.clone(), read(":other"), read("17"), read(":line"), read("91")]);
        // Reader equality ignores locations/key metadata, but association must
        // retain the first key object including its complete source metadata.
        assert_eq!(output[0].span, inner_key.span);
        assert_eq!(output[0].metadata, inner_key.metadata);
        assert_eq!(output[1].metadata, outer_value.metadata);
        assert_eq!(merged_metadata_pairs_metered(&form, false, &mut |_| Ok(())).unwrap(), output[..4]);
        for stop in [0, calls / 2, calls - 1] {
            let mut current = 0;
            let error = reader_metadata_pairs_metered(&form, &mut |_| {
                let call = current; current += 1;
                if call == stop { Err(fail(4..9, "metadata merge cancelled")) } else { Ok(()) }
            }).unwrap_err();
            assert_eq!(error.message, "metadata merge cancelled");
            assert_eq!(form, original);
            assert_eq!(reader_metadata_pairs_metered(&form, &mut |_| Ok(())).unwrap(), output);
        }
    }

    #[test]
    fn deep_metadata_keys_preserve_precedence_and_exhaust_meter_without_a_depth_cap() {
        let mut key = Form { span: 0..1, metadata: vec![], kind: Kind::Number(42.0) };
        for _ in 0..128 {
            key = Form { span: 0..1, metadata: vec![], kind: Kind::Vector(vec![key]) };
        }
        let mut form = suss_reader::forms::read_forms("value").unwrap().remove(0);
        form.metadata = [17.0, 19.0].into_iter().map(|value| Form {
            span: 0..1, metadata: vec![],
            kind: Kind::Map(vec![key.clone(), Form { span: 0..1, metadata: vec![], kind: Kind::Number(value) }]),
        }).collect();
        let mut charged = 0;
        let pairs = reader_metadata_pairs_metered(&form, &mut |work| { charged += work; Ok(()) }).unwrap();
        assert!(charged > 128);
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0], key);
        assert_eq!(pairs[1].kind, Kind::Number(17.0), "outer prefix replaces only the existing value");
        let error = reader_metadata_pairs_metered(&form, &mut |_| Err(fail(0..1, "test fuel exhausted"))).unwrap_err();
        assert!(error.message.contains("fuel exhausted"));
        assert_eq!(reader_metadata_pairs_metered(&form, &mut |_| Ok(())).unwrap(), pairs);
    }
    #[test]
    fn metadata_reader_keys_compare_canonical_duplicate_map_and_set_data() {
        for (a, b, equal) in [
            ("{:a 1 :a 1}", "{:a 1 :b 1}", false),
            ("{:a 1 :a 2}", "{:a 2}", true),
            ("{:a 1 :a 2}", "{:a 1}", false),
            ("#{1 1}", "#{1 2}", false),
            ("#{1 1}", "#{1}", true),
            ("#{[1 2] (1 2)}", "#{(1 2)}", true),
            ("{:a #{1 1} :b [1 2]}", "{:b (1 2) :a #{1}}", true),
        ] {
            let a = suss_reader::forms::read_forms(a).unwrap().remove(0);
            let b = suss_reader::forms::read_forms(b).unwrap().remove(0);
            assert_eq!(reader_key_equal(&a, &b, 0, &mut 1_048_576).unwrap(), equal);
            assert_eq!(reader_key_equal(&b, &a, 0, &mut 1_048_576).unwrap(), equal);
        }
        let value = suss_reader::forms::read_forms("#{1 1}").unwrap().remove(0);
        assert!(reader_key_equal(&value, &value, 0, &mut 1).is_err());
        assert!(reader_key_equal(&value, &value, 64, &mut 1_048_576).unwrap());
    }
}
