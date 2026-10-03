// Copyright (c) Nicola Mometto, Rich Hickey & contributors.
// The use and distribution terms for this software are covered by the
// Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
// which can be found in the file epl-v10.html at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.
// You must not remove this notice, or any other, from this software.
// SPDX-License-Identifier: EPL-1.0

//! Adapted indexing metadata and read-meta precedence from pinned tools.reader,
//! reader.clj181–239,308–328,372–406 at c4295f303100bbf5afac449242d30bca1126f1a1.
//! Source SHA256: 614e857a4d92e22edbaf38e4e87c3f499e8703fb7f930c23e1df136535cc4597.
use super::{Form, Kind};
use crate::Keyword;
use std::{collections::HashMap, ops::Range};

pub(super) type Carets = HashMap<(usize, usize), (usize, usize)>;

pub(super) struct Positions {
    starts: Vec<usize>,
}
impl Positions {
    pub(super) fn new(source: &str) -> Self {
        let mut starts = vec![0];
        let mut chars = source.char_indices().peekable();
        while let Some((offset, ch)) = chars.next() {
            match ch {
                '\r' => {
                    let mut end = offset + 1;
                    if chars.peek().is_some_and(|(_, ch)| *ch == '\n') {
                        chars.next();
                        end += 1;
                    }
                    starts.push(end);
                }
                '\n' => starts.push(offset + 1),
                _ => (),
            }
        }
        Self { starts }
    }
    pub(super) fn position(&self, source: &str, offset: usize) -> (usize, usize) {
        let line = self.starts.partition_point(|start| *start <= offset) - 1;
        (
            line + 1,
            source[self.starts[line]..offset].encode_utf16().count() + 1,
        )
    }
}

fn data(span: &Range<usize>, kind: Kind) -> Form {
    Form {
        span: span.clone(),
        metadata: vec![],
        kind,
    }
}
fn pair(span: &Range<usize>, key: &str, value: Kind) -> [Form; 2] {
    [
        data(
            span,
            Kind::Keyword(Keyword {
                namespace: None,
                name: key.into(),
            }),
        ),
        data(span, value),
    ]
}

pub(super) fn attach(source: &str, positions: &Positions, file: Option<&str>, form: &mut Form) {
    if !matches!(
        &form.kind,
        Kind::List(_) | Kind::Vector(_) | Kind::Map(_) | Kind::Set(_) | Kind::Symbol(_)
    ) {
        return;
    }
    // Pinned read-symbol returns the slash special symbol without indexing meta.
    if matches!(&form.kind, Kind::Symbol(symbol) if symbol.namespace.is_none() && symbol.name == "/")
    {
        return;
    }
    let (line, column) = positions.position(source, form.span.start);
    let (end_line, end_column) = positions.position(source, form.span.end);
    let mut entries = vec![];
    for (key, value) in [
        ("line", line),
        ("column", column),
        ("end-line", end_line),
        ("end-column", end_column),
    ] {
        entries.extend(pair(&form.span, key, Kind::Number(value as f64)));
    }
    if let Some(file) = file {
        entries.extend(pair(
            &form.span,
            "file",
            Kind::String(file.encode_utf16().collect()),
        ));
    }
    form.metadata.push(data(&form.span, Kind::Map(entries)));
}

pub(super) fn list_prefixes(forms: &mut [Form], carets: &Carets) {
    for form in forms {
        list_prefixes(&mut form.metadata, carets);
        if matches!(form.kind, Kind::List(_)) {
            for prefix in &mut form.metadata {
                if let Some(&(line, column)) = carets.get(&(prefix.span.start, prefix.span.end)) {
                    let mut entries = vec![];
                    entries.extend(pair(&prefix.span, "line", Kind::Number(line as f64)));
                    entries.extend(pair(&prefix.span, "column", Kind::Number(column as f64)));
                    match &prefix.kind {
                        Kind::Map(items) => entries.extend(items.clone()),
                        Kind::Keyword(_) => {
                            entries.extend([prefix.clone(), data(&prefix.span, Kind::Bool(true))])
                        }
                        Kind::Symbol(_) | Kind::String(_) => {
                            entries.push(data(
                                &prefix.span,
                                Kind::Keyword(Keyword {
                                    namespace: None,
                                    name: "tag".into(),
                                }),
                            ));
                            entries.push(prefix.clone());
                        }
                        _ => unreachable!("reader validated metadata prefix"),
                    }
                    prefix.kind = Kind::Map(entries);
                }
            }
        }
        match &mut form.kind {
            Kind::List(items) | Kind::Vector(items) | Kind::Map(items) | Kind::Set(items) => {
                list_prefixes(items, carets)
            }
            _ => (),
        }
    }
}
