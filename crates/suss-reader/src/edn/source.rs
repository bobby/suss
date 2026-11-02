//! Reader forms as EDN data (#299 step 2).
//!
//! A form becomes the [`Value`] it reads as, with its prefix metadata merged
//! as tools.reader merges it. Source positions are not metadata (spec §5), so
//! [`span_tree`] returns them as a parallel tree, in the order the value
//! prints: map entries and set items in canonical order.
use super::{Error, Value};
use crate::forms::{Form, Kind};
use crate::Symbol;
use std::collections::{BTreeMap, BTreeSet};

/// The tag of a string that is not valid Unicode, such as one holding a lone
/// surrogate: its UTF-16 code units, `#suss/utf16 [55296]`.
pub const UTF16_TAG: &str = "utf16";

fn keyword(name: &str) -> Value {
    Value::keyword(name)
}

/// The value a selected form reads as. Reader conditionals, discards and
/// prefixes must already be selected, as they are in analyzed forms.
pub fn form_to_value(form: &Form) -> Result<Value, Error> {
    stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
        let value = match &form.kind {
            Kind::Nil => Value::Nil,
            Kind::Bool(b) => Value::Bool(*b),
            Kind::Number(n) => Value::Number(*n),
            Kind::String(units) => string_value(units),
            Kind::Symbol(s) => Value::Symbol(s.clone()),
            Kind::Keyword(k) => Value::Keyword(k.clone()),
            Kind::List(items) => Value::List(values(items)?),
            Kind::Vector(items) => Value::Vector(values(items)?),
            Kind::Set(items) => Value::Set(values(items)?.into_iter().collect::<BTreeSet<_>>()),
            Kind::Map(entries) => {
                if entries.len() % 2 != 0 {
                    return Err(Error::new("Map literal must contain an even number of forms"));
                }
                let mut map = BTreeMap::new();
                for pair in entries.chunks_exact(2) {
                    map.insert(form_to_value(&pair[0])?, form_to_value(&pair[1])?);
                }
                Value::Map(map)
            }
            Kind::Tagged { tag, form } => Value::Tagged(tag.clone(), Box::new(form_to_value(form)?)),
            Kind::Conditional(_) | Kind::SplicingConditional(_) | Kind::Discard(_) | Kind::Prefix { .. } => {
                return Err(Error::new("Reader syntax must be selected before conversion to data"));
            }
        };
        let meta = metadata(form)?;
        Ok(if meta.is_empty() { value } else { Value::Meta(Box::new(value), meta) })
    })
}

fn values(items: &[Form]) -> Result<Vec<Value>, Error> {
    items.iter().map(form_to_value).collect()
}

/// A Suss string: an EDN string when it is valid Unicode, otherwise its
/// UTF-16 units under [`UTF16_TAG`].
pub fn string_value(units: &[u16]) -> Value {
    match String::from_utf16(units) {
        Ok(text) => Value::String(text),
        Err(_) => Value::Tagged(
            Symbol { namespace: Some("suss".into()), name: UTF16_TAG.into() },
            Box::new(Value::Vector(units.iter().map(|&u| Value::Number(f64::from(u))).collect())),
        ),
    }
}

/// The merged metadata of a form's prefixes. As in tools.reader, the
/// innermost prefix applies first and outer prefixes replace its values.
pub fn metadata(form: &Form) -> Result<BTreeMap<Value, Value>, Error> {
    let mut meta = BTreeMap::new();
    for prefix in form.metadata.iter().rev() {
        match &prefix.kind {
            Kind::Map(entries) if entries.len() % 2 == 0 => {
                for pair in entries.chunks_exact(2) {
                    meta.insert(form_to_value(&pair[0])?, form_to_value(&pair[1])?);
                }
            }
            Kind::Keyword(k) => {
                meta.insert(Value::Keyword(k.clone()), Value::Bool(true));
            }
            Kind::Symbol(_) | Kind::String(_) => {
                meta.insert(keyword("tag"), form_to_value(prefix)?);
            }
            _ => return Err(Error::new("Metadata must be a map, keyword, symbol or string")),
        }
    }
    Ok(meta)
}

/// A form that reads as `value`, without source positions. Metadata becomes
/// one map prefix; a `#suss/utf16` string becomes the string it encodes.
pub fn value_to_form(value: &Value) -> Form {
    stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
        let form = |kind| Form { span: 0..0, metadata: Vec::new(), kind };
        match value {
            Value::Nil => form(Kind::Nil),
            Value::Bool(b) => form(Kind::Bool(*b)),
            Value::Number(n) => form(Kind::Number(*n)),
            Value::String(s) => form(Kind::String(s.encode_utf16().collect())),
            Value::Keyword(k) => form(Kind::Keyword(k.clone())),
            Value::Symbol(s) => form(Kind::Symbol(s.clone())),
            Value::List(items) => form(Kind::List(items.iter().map(value_to_form).collect())),
            Value::Vector(items) => form(Kind::Vector(items.iter().map(value_to_form).collect())),
            Value::Set(items) => form(Kind::Set(items.iter().map(value_to_form).collect())),
            Value::Map(map) => form(Kind::Map(
                map.iter().flat_map(|(k, v)| [value_to_form(k), value_to_form(v)]).collect(),
            )),
            Value::Tagged(tag, inner) => match (tag.namespace.as_deref(), tag.name.as_str(), &**inner) {
                (Some("suss"), UTF16_TAG, Value::Vector(units))
                    if units.iter().all(|u| matches!(u, Value::Number(n) if n.fract() == 0.0 && (0.0..=65535.0).contains(n))) =>
                {
                    form(Kind::String(
                        units.iter().map(|u| match u { Value::Number(n) => *n as u16, _ => 0 }).collect(),
                    ))
                }
                _ => form(Kind::Tagged { tag: tag.clone(), form: Box::new(value_to_form(inner)) }),
            },
            Value::Meta(inner, meta) => {
                let mut out = value_to_form(inner);
                out.metadata = vec![form(Kind::Map(
                    meta.iter().flat_map(|(k, v)| [value_to_form(k), value_to_form(v)]).collect(),
                ))];
                out
            }
        }
    })
}

/// Source text that reads as `form`, keeping the order of its map and set
/// entries, which analysis follows. Metadata prints as one `^{...}` prefix.
pub fn print_form(form: &Form) -> Result<String, Error> {
    let mut out = String::new();
    write_form(form, &mut out)?;
    Ok(out)
}

fn write_form(form: &Form, out: &mut String) -> Result<(), Error> {
    stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
        let meta = metadata(form)?;
        if !meta.is_empty() {
            out.push('^');
            out.push_str(&super::print(&Value::Map(meta))?);
            out.push(' ');
        }
        let items = |open: &str, items: &[Form], close: char, out: &mut String| -> Result<(), Error> {
            out.push_str(open);
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                write_form(item, out)?;
            }
            out.push(close);
            Ok(())
        };
        match &form.kind {
            Kind::List(forms) => items("(", forms, ')', out),
            Kind::Vector(forms) => items("[", forms, ']', out),
            Kind::Set(forms) => items("#{", forms, '}', out),
            Kind::Map(forms) => items("{", forms, '}', out),
            Kind::Tagged { tag, form } => {
                out.push('#');
                out.push_str(&super::print(&Value::Symbol(tag.clone()))?);
                out.push(' ');
                write_form(form, out)
            }
            Kind::String(units) if String::from_utf16(units).is_err() => {
                out.push('"');
                for &unit in units {
                    match char::from_u32(u32::from(unit)) {
                        Some(c) if unit < 0xd800 && !c.is_control() && c != '"' && c != '\\' => out.push(c),
                        _ => out.push_str(&format!("\\u{unit:04x}")),
                    }
                }
                out.push('"');
                Ok(())
            }
            _ => {
                let bare = Form { span: form.span.clone(), metadata: Vec::new(), kind: form.kind.clone() };
                out.push_str(&super::print(&form_to_value(&bare)?)?);
                Ok(())
            }
        }
    })
}

/// Line and column (both from 1, columns in UTF-16 code units, as Java
/// counts characters) of byte offsets in a source text, as tools.reader
/// reports them.
pub struct LineIndex<'a> {
    text: &'a str,
    starts: Vec<usize>,
}

impl<'a> LineIndex<'a> {
    pub fn new(text: &'a str) -> Self {
        let starts = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(i, _)| i + 1))
            .collect();
        LineIndex { text, starts }
    }
    pub fn position(&self, offset: usize) -> (usize, usize) {
        let offset = offset.min(self.text.len());
        let line = self.starts.partition_point(|&start| start <= offset);
        let start = self.starts[line - 1];
        let column = self.text.get(start..offset).map_or(offset - start, |s| s.encode_utf16().count());
        (line, column + 1)
    }
}

/// The source positions of a form, in a tree parallel to its value:
/// `{:start :end}` byte offsets, `:line :column :end-line :end-column` when
/// `lines` is given, `:meta` for its prefixes, and its children under
/// `:items` (lists, vectors and sets), `:entries` (`[key value]` pairs of a
/// map) or `:form` (a tagged literal's form). Set items and map entries are in
/// the canonical order in which the value prints.
pub fn span_tree(form: &Form, lines: Option<&LineIndex>) -> Result<Value, Error> {
    stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
        let mut tree = position(&form.span, lines);
        if !form.metadata.is_empty() {
            tree.insert(
                keyword("meta"),
                Value::Vector(form.metadata.iter().map(|m| span_tree(m, lines)).collect::<Result<_, _>>()?),
            );
        }
        match &form.kind {
            Kind::List(items) | Kind::Vector(items) => {
                tree.insert(keyword("items"), trees(items.iter(), lines)?);
            }
            Kind::Set(items) => {
                let mut sorted = items.iter().map(|i| Ok((form_to_value(i)?, i))).collect::<Result<Vec<_>, Error>>()?;
                sorted.sort_by(|a, b| a.0.cmp(&b.0));
                tree.insert(keyword("items"), trees(sorted.into_iter().map(|(_, i)| i), lines)?);
            }
            Kind::Map(entries) => {
                let mut sorted = entries
                    .chunks_exact(2)
                    .map(|pair| Ok((form_to_value(&pair[0])?, pair)))
                    .collect::<Result<Vec<_>, Error>>()?;
                sorted.sort_by(|a, b| a.0.cmp(&b.0));
                let entries = sorted
                    .into_iter()
                    .map(|(_, pair)| Ok(Value::Vector(vec![span_tree(&pair[0], lines)?, span_tree(&pair[1], lines)?])))
                    .collect::<Result<Vec<_>, Error>>()?;
                tree.insert(keyword("entries"), Value::Vector(entries));
            }
            Kind::Tagged { form, .. } => {
                tree.insert(keyword("form"), span_tree(form, lines)?);
            }
            _ => {}
        }
        Ok(Value::Map(tree))
    })
}

fn trees<'f>(items: impl Iterator<Item = &'f Form>, lines: Option<&LineIndex>) -> Result<Value, Error> {
    Ok(Value::Vector(items.map(|i| span_tree(i, lines)).collect::<Result<_, _>>()?))
}

/// `{:start :end}` and, with `lines`, `:line :column :end-line :end-column`.
pub fn position(span: &std::ops::Range<usize>, lines: Option<&LineIndex>) -> BTreeMap<Value, Value> {
    let number = |n: usize| Value::Number(n as f64);
    let mut map = BTreeMap::from([(keyword("start"), number(span.start)), (keyword("end"), number(span.end))]);
    if let Some(lines) = lines {
        let (line, column) = lines.position(span.start);
        let (end_line, end_column) = lines.position(span.end);
        map.extend([
            (keyword("line"), number(line)),
            (keyword("column"), number(column)),
            (keyword("end-line"), number(end_line)),
            (keyword("end-column"), number(end_column)),
        ]);
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edn::{Options, print, read_one};

    fn read(source: &str) -> Form {
        crate::forms::resolve_conditionals(crate::forms::read_forms(source).unwrap()).unwrap().remove(0)
    }

    #[test]
    fn forms_convert_to_the_values_they_read_as() {
        for source in [
            "nil", "true", "1.5", "\"a\\nb\"", "\\c", "sym", "ns/sym", ":k", ":ns/k",
            "(a [b {:c #{d}}])", "#queue [1]",
        ] {
            let value = form_to_value(&read(source)).unwrap();
            let expected = read_one(source, &Options::preserving_tags());
            assert_eq!(print(&value).unwrap(), print(&expected.unwrap()).unwrap(), "{source}");
            assert_eq!(form_to_value(&value_to_form(&value)).unwrap(), value, "{source}");
        }
    }

    #[test]
    fn metadata_merges_inner_first_and_round_trips() {
        let value = form_to_value(&read("^:a ^{:a 2 :b 3} ^String [x]")).unwrap();
        assert_eq!(print(&value).unwrap(), "^{:a true, :b 3, :tag String} [x]");
        let back = form_to_value(&value_to_form(&value)).unwrap();
        assert_eq!(print(&back).unwrap(), print(&value).unwrap());
    }

    #[test]
    fn printed_forms_keep_entry_order_and_read_back() {
        for source in ["{:b 1 :a [#{3 2} \"\\ud800x\"]}", "^:m (f ^{:t 1} x)", "#queue [1 2]"] {
            let form = read(source);
            let printed = print_form(&form).unwrap();
            let again = read(&printed);
            assert_eq!(form_to_value(&again).unwrap(), form_to_value(&form).unwrap(), "{source}");
            assert_eq!(print_form(&again).unwrap(), printed, "{source}");
        }
        assert_eq!(print_form(&read("{:b 1 :a 2}")).unwrap(), "{:b 1 :a 2}");
    }

    #[test]
    fn lone_surrogates_are_tagged_utf16() {
        let value = form_to_value(&read("\"\\ud800x\"")).unwrap();
        assert_eq!(print(&value).unwrap(), "#suss/utf16 [55296 120]");
        let form = value_to_form(&value);
        assert!(matches!(form.kind, Kind::String(ref u) if u == &[0xd800, 0x78]));
    }

    #[test]
    fn span_trees_follow_printed_order_with_lines_and_columns() {
        let text = "x\n {:b 1 :a ^:m [é 2]}";
        let form = read(&text[2..]);
        let shifted = |span: &std::ops::Range<usize>| span.start + 2..span.end + 2;
        let mut located = form.clone();
        fn shift(form: &mut Form, by: &dyn Fn(&std::ops::Range<usize>) -> std::ops::Range<usize>) {
            form.span = by(&form.span);
            form.metadata.iter_mut().for_each(|m| shift(m, by));
            if let Kind::Map(items) | Kind::Vector(items) = &mut form.kind {
                items.iter_mut().for_each(|i| shift(i, by));
            }
        }
        shift(&mut located, &shifted);
        let lines = LineIndex::new(text);
        let tree = span_tree(&located, Some(&lines)).unwrap();
        assert_eq!(
            print(&tree).unwrap(),
            "{:column 2, :end 23, :end-column 21, :end-line 2, :entries [\
             [{:column 8, :end 11, :end-column 10, :end-line 2, :line 2, :start 9} \
             {:column 11, :end 22, :end-column 20, :end-line 2, :items [\
             {:column 16, :end 19, :end-column 17, :end-line 2, :line 2, :start 17} \
             {:column 18, :end 21, :end-column 19, :end-line 2, :line 2, :start 20}], :line 2, \
             :meta [{:column 12, :end 15, :end-column 14, :end-line 2, :line 2, :start 13}], :start 12}] \
             [{:column 3, :end 6, :end-column 5, :end-line 2, :line 2, :start 4} \
             {:column 6, :end 8, :end-column 7, :end-line 2, :line 2, :start 7}]], :line 2, :start 3}"
        );
    }
}
