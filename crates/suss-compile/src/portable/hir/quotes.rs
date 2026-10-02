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
        if depth >= 64 {
            return Err(fail(form.span.clone(), "Quoted data nesting exceeds 64"));
        }
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
            Kind::Nil | Kind::Bool(_) | Kind::Number(_) | Kind::String(_) => self.form(form),
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
            Kind::List(items) => {
                let symbol = suss_reader::Symbol {
                    namespace: Some("suss.core".into()),
                    name: "list".into(),
                };
                let (kind, ty) = self.global_value(&symbol, form.span.clone())?;
                let callee = Hir {
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
                "Quoted vector/map/set data needs persistent collection types, not yet implemented",
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
            span: form.span.clone(),
            metadata: vec![],
            kind,
            ty,
        };
        Ok(Hir {
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
        let mut result = self.fresh_binding(form, empty);
        let mut bindings = vec![result.clone()];
        let mut retained = reader_data;
        // Pinned tools.reader/read-meta merges inner metadata first, then outer.
        // Actual source -assoc supplies key equality and last-value semantics.
        for prefix in form.metadata.iter().rev() {
            let entries = match &prefix.kind {
                Kind::Map(entries) if entries.len() % 2 == 0 => entries.clone(),
                Kind::Keyword(_) => vec![
                    prefix.clone(),
                    Form {
                        span: prefix.span.clone(),
                        metadata: vec![],
                        kind: Kind::Bool(true),
                    },
                ],
                Kind::Symbol(_) | Kind::String(_) => vec![
                    Form {
                        span: prefix.span.clone(),
                        metadata: vec![],
                        kind: Kind::Keyword(suss_reader::Keyword {
                            namespace: None,
                            name: "tag".into(),
                        }),
                    },
                    prefix.clone(),
                ],
                _ => {
                    return Err(fail(
                        prefix.span.clone(),
                        "Invalid metadata: expected a map, keyword, symbol or string",
                    ));
                }
            };
            for pair in entries.chunks_exact(2) {
                // cljs.analyzer/elide-irrelevant-meta, pinned source4435-4452.
                let irrelevant = matches!(&pair[0].kind, Kind::Keyword(key) if
                    (key.namespace.is_none() && matches!(key.name.as_str(), "file" | "line" | "column" | "end-column" | "end-line" | "source"))
                    || (key.namespace.as_deref() == Some("cljs.analyzer") && key.name == "analyzed"));
                if !reader_data && irrelevant {
                    continue;
                }
                let key = self.quote_data_impl(&pair[0], depth + 2, reader_data)?;
                let item = self.quote_data_impl(&pair[1], depth + 2, reader_data)?;
                let previous = self.local(form, result.id);
                let next = self.data_core_call(form, "-assoc", vec![previous, key, item])?;
                result = self.fresh_binding(form, next);
                bindings.push(result.clone());
                retained = true;
            }
        }
        if !retained {
            return Ok(value);
        }
        let result = Hir {
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

// Keep the pinned literal hash independent of redefinitions of public hash cells.
// The prototype's xxHash constant helpers have a different contract and are unused.
fn identifier_hash(namespace: Option<&str>, name: &str, keyword: bool) -> i32 {
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
