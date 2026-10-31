//! Canonical EDN for tooling data (#299): a value model, a deterministic
//! printer and the `suss.edn` reading contract, following `clojure.edn`.
//!
//! Reading uses the forms reader in its EDN mode, so code-only syntax (quote
//! sugar, syntax quote, `#()`, `#'`, reader conditionals) is an error at its
//! span. Tags are an error unless built in (`#queue`) or handled by
//! [`Options::readers`] or [`Options::default`]. Input size is bounded by the
//! reader's work allowance and nesting by [`Options::max_depth`].
//!
//! Printing is canonical: map and set entries in [`Value`]'s total order,
//! shortest round-trip numbers and one string escaping, so equal values print
//! byte-identically in every process.
use crate::forms::{self, Form, Kind};
use crate::{Keyword, ParseError, Symbol};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet, HashMap},
    ops::Range,
};

pub mod serde;
pub use self::serde::{from_str, from_value, to_string, to_string_pretty, to_value};

/// EDN data. Characters read as one-character strings, as Suss represents
/// them. Equality and order are structural and total: numbers compare by
/// their binary64 bits' total order (so `##NaN` equals itself and `-0.0` is
/// not `0.0`), lists and vectors differ, and metadata is ignored, as in
/// Clojure.
#[derive(Debug, Clone)]
pub enum Value {
    Nil,
    Bool(bool),
    Number(f64),
    String(String),
    Keyword(Keyword),
    Symbol(Symbol),
    List(Vec<Value>),
    Vector(Vec<Value>),
    Map(BTreeMap<Value, Value>),
    Set(BTreeSet<Value>),
    Tagged(Symbol, Box<Value>),
    /// A symbol, collection or tagged value with non-empty metadata.
    Meta(Box<Value>, BTreeMap<Value, Value>),
}

impl Value {
    /// A keyword from its printed name, `name` or `ns/name`, without the colon.
    pub fn keyword(name: &str) -> Self {
        Value::Keyword(Keyword::parse(name))
    }
    /// This value without its metadata.
    pub fn unmeta(&self) -> &Value {
        match self {
            Value::Meta(value, _) => value,
            value => value,
        }
    }
    fn rank(&self) -> u8 {
        match self {
            Value::Nil => 0,
            Value::Bool(_) => 1,
            Value::Number(_) => 2,
            Value::String(_) => 3,
            Value::Keyword(_) => 4,
            Value::Symbol(_) => 5,
            Value::List(_) => 6,
            Value::Vector(_) => 7,
            Value::Map(_) => 8,
            Value::Set(_) => 9,
            Value::Tagged(..) => 10,
            Value::Meta(value, _) => value.rank(),
        }
    }
}

fn symbol_cmp(a: &Symbol, b: &Symbol) -> Ordering {
    (&a.namespace, &a.name).cmp(&(&b.namespace, &b.name))
}

impl Ord for Value {
    fn cmp(&self, other: &Self) -> Ordering {
        stacker::maybe_grow(64 * 1024, 1024 * 1024, || match (self.unmeta(), other.unmeta()) {
            (Value::Bool(a), Value::Bool(b)) => a.cmp(b),
            (Value::Number(a), Value::Number(b)) => a.total_cmp(b),
            (Value::String(a), Value::String(b)) => a.cmp(b),
            (Value::Keyword(a), Value::Keyword(b)) => {
                (&a.namespace, &a.name).cmp(&(&b.namespace, &b.name))
            }
            (Value::Symbol(a), Value::Symbol(b)) => symbol_cmp(a, b),
            (Value::List(a), Value::List(b)) | (Value::Vector(a), Value::Vector(b)) => a.cmp(b),
            (Value::Map(a), Value::Map(b)) => a.cmp(b),
            (Value::Set(a), Value::Set(b)) => a.cmp(b),
            (Value::Tagged(t, a), Value::Tagged(u, b)) => {
                symbol_cmp(t, u).then_with(|| a.cmp(b))
            }
            (a, b) => a.rank().cmp(&b.rank()),
        })
    }
}
impl PartialOrd for Value {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for Value {}

/// A reading or printing failure. Reading failures carry the byte span of
/// the offending source.
#[derive(Debug, Clone, PartialEq)]
pub struct Error {
    pub message: String,
    pub span: Option<Range<usize>>,
}
impl Error {
    pub fn new(message: impl Into<String>) -> Self {
        Error {
            message: message.into(),
            span: None,
        }
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.span {
            Some(span) => write!(f, "{} at bytes {}..{}", self.message, span.start, span.end),
            None => f.write_str(&self.message),
        }
    }
}
impl std::error::Error for Error {}
impl From<ParseError> for Error {
    fn from(error: ParseError) -> Self {
        Error {
            message: error.message,
            span: Some(error.span),
        }
    }
}

// ---------------------------------------------------------------------------
// Printing

/// Print `value` canonically on one line, with `, ` between map entries as
/// Clojure's `pr` does. Fails only for values that would not read back: an
/// unreadable symbol, keyword or tag, or metadata on a value that cannot
/// carry it.
pub fn print(value: &Value) -> Result<String, Error> {
    let mut out = String::new();
    write(&mut out, value)?;
    Ok(out)
}

/// Print `value` canonically, breaking collections wider than 80 columns
/// into one entry per line. The layout is a function of the value alone.
pub fn print_pretty(value: &Value) -> Result<String, Error> {
    let mut out = String::new();
    pretty(&mut out, value, 0)?;
    Ok(out)
}

const WIDTH: usize = 80;

fn write(out: &mut String, value: &Value) -> Result<(), Error> {
    stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
        match value {
            Value::Nil => out.push_str("nil"),
            Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Value::Number(x) => out.push_str(&number(*x)),
            Value::String(s) => string(out, s),
            Value::Keyword(k) => out.push_str(&keyword_text(k)?),
            Value::Symbol(s) => out.push_str(&symbol_text(s)?),
            Value::List(items) => sequence(out, "(", items.iter(), ")")?,
            Value::Vector(items) => sequence(out, "[", items.iter(), "]")?,
            Value::Set(items) => sequence(out, "#{", items.iter(), "}")?,
            Value::Map(entries) => {
                out.push('{');
                for (index, (key, value)) in entries.iter().enumerate() {
                    if index > 0 {
                        out.push_str(", ");
                    }
                    write(out, key)?;
                    out.push(' ');
                    write(out, value)?;
                }
                out.push('}');
            }
            Value::Tagged(tag, value) => {
                out.push('#');
                out.push_str(&tag_text(tag)?);
                out.push(' ');
                write(out, value)?;
            }
            Value::Meta(value, meta) => {
                check_meta_target(value)?;
                out.push('^');
                write(out, &Value::Map(meta.clone()))?;
                out.push(' ');
                write(out, value)?;
            }
        }
        Ok(())
    })
}
fn sequence<'v>(
    out: &mut String,
    open: &str,
    items: impl Iterator<Item = &'v Value>,
    close: &str,
) -> Result<(), Error> {
    out.push_str(open);
    for (index, item) in items.enumerate() {
        if index > 0 {
            out.push(' ');
        }
        write(out, item)?;
    }
    out.push_str(close);
    Ok(())
}

fn pretty(out: &mut String, value: &Value, column: usize) -> Result<(), Error> {
    stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
        let flat = print(value)?;
        if column + flat.chars().count() <= WIDTH {
            out.push_str(&flat);
            return Ok(());
        }
        let items = |out: &mut String, open: &str, items: Vec<&Value>, close: &str| {
            out.push_str(open);
            let inner = column + open.len();
            for (index, item) in items.into_iter().enumerate() {
                if index > 0 {
                    newline(out, inner);
                }
                pretty(out, item, inner)?;
            }
            out.push_str(close);
            Ok(())
        };
        match value {
            Value::List(x) => items(out, "(", x.iter().collect(), ")"),
            Value::Vector(x) => items(out, "[", x.iter().collect(), "]"),
            Value::Set(x) => items(out, "#{", x.iter().collect(), "}"),
            Value::Map(entries) => {
                out.push('{');
                for (index, (key, value)) in entries.iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                        newline(out, column + 1);
                    }
                    let key = print(key)?;
                    out.push_str(&key);
                    out.push(' ');
                    pretty(out, value, column + 1 + key.chars().count() + 1)?;
                }
                out.push('}');
                Ok(())
            }
            Value::Tagged(tag, value) => {
                let tag = tag_text(tag)?;
                out.push('#');
                out.push_str(&tag);
                out.push(' ');
                pretty(out, value, column + tag.chars().count() + 2)
            }
            Value::Meta(value, meta) => {
                check_meta_target(value)?;
                out.push('^');
                pretty(out, &Value::Map(meta.clone()), column + 1)?;
                newline(out, column);
                pretty(out, value, column)
            }
            _ => {
                out.push_str(&flat);
                Ok(())
            }
        }
    })
}
fn newline(out: &mut String, column: usize) {
    out.push('\n');
    out.extend(std::iter::repeat_n(' ', column));
}

fn check_meta_target(value: &Value) -> Result<(), Error> {
    match value {
        Value::Symbol(_)
        | Value::List(_)
        | Value::Vector(_)
        | Value::Map(_)
        | Value::Set(_)
        | Value::Tagged(..) => Ok(()),
        _ => Err(Error::new("Metadata requires a symbol, collection or tagged value")),
    }
}

/// The canonical spelling of a binary64: integers below 10^16 without a
/// fraction, `-0.0` with its sign, `##Inf`, `##-Inf` and `##NaN`, and
/// otherwise Rust's shortest round-trip form, such as `0.1` or `1e300`.
pub fn number(x: f64) -> String {
    if x.is_nan() {
        "##NaN".into()
    } else if x.is_infinite() {
        if x > 0.0 { "##Inf" } else { "##-Inf" }.into()
    } else if x == 0.0 && x.is_sign_negative() {
        "-0.0".into()
    } else if x.fract() == 0.0 && x.abs() < 1e16 {
        (x as i64).to_string()
    } else {
        format!("{x:?}")
    }
}

fn string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if c < ' ' || c == '\u{7f}' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// A token the reader reads back as one symbol or keyword body.
fn readable_token(text: &str) -> bool {
    !text.is_empty()
        && !text.chars().any(forms::delimiter)
        && !text.starts_with(['#', '\'', ':'])
        && !forms::numeric_start(text)
        && forms::valid_symbol(text)
}
fn qualified(namespace: &Option<String>, name: &str) -> String {
    match namespace {
        Some(namespace) => format!("{namespace}/{name}"),
        None => name.to_owned(),
    }
}
fn symbol_text(symbol: &Symbol) -> Result<String, Error> {
    let text = qualified(&symbol.namespace, &symbol.name);
    if readable_token(&text)
        && !matches!(text.as_str(), "nil" | "true" | "false")
        && Symbol::parse(&text) == *symbol
    {
        Ok(text)
    } else {
        Err(Error::new(format!("Symbol {text:?} cannot be printed readably")))
    }
}
fn keyword_text(keyword: &Keyword) -> Result<String, Error> {
    let text = qualified(&keyword.namespace, &keyword.name);
    // Keyword bodies may start with a digit, as `:1` reads.
    let body_ok = !text.is_empty()
        && !text.chars().any(forms::delimiter)
        && !text.starts_with(':')
        && forms::valid_symbol(&text);
    if body_ok && Keyword::parse(&text) == *keyword {
        Ok(format!(":{text}"))
    } else {
        Err(Error::new(format!("Keyword {text:?} cannot be printed readably")))
    }
}
fn tag_text(tag: &Symbol) -> Result<String, Error> {
    let text = symbol_text(tag)?;
    if text.starts_with(char::is_alphabetic) {
        Ok(text)
    } else {
        Err(Error::new(format!("Tag {text:?} must start with a letter")))
    }
}

// ---------------------------------------------------------------------------
// Reading

/// A reader for one tag: its value in, the tagged value out.
pub type TagReader = dyn Fn(Value) -> Result<Value, String>;
/// The fallback for tags with no reader, as `clojure.edn`'s `:default`.
pub type DefaultReader = dyn Fn(&Symbol, Value) -> Result<Value, String>;

/// Reading options, following `clojure.edn`'s `:readers` and `:default`.
pub struct Options {
    /// Reader work units, about one per source character and one per form.
    pub work_allowance: u64,
    /// Deepest collection, tag or metadata nesting accepted.
    pub max_depth: usize,
    /// Readers for tags, taking precedence over the built-in `#queue`.
    pub readers: HashMap<Symbol, Box<TagReader>>,
    /// The reader for any other tag. Without one, other tags are an error.
    pub default: Option<Box<DefaultReader>>,
}
impl Default for Options {
    fn default() -> Self {
        Options {
            work_allowance: forms::DEFAULT_WORK_ALLOWANCE,
            max_depth: 1024,
            readers: HashMap::new(),
            default: None,
        }
    }
}
impl Options {
    /// Options that keep every unknown tag as [`Value::Tagged`].
    pub fn preserving_tags() -> Self {
        Options {
            default: Some(Box::new(|tag, value| Ok(Value::Tagged(tag.clone(), Box::new(value))))),
            ..Options::default()
        }
    }
}

/// Read every EDN value in `source`, in order.
pub fn read_all(source: &str, options: &Options) -> Result<Vec<Value>, Error> {
    let mut work = options.work_allowance;
    let meter = &mut |units| forms::allowance(&mut work, units);
    let forms = forms::read_edn_forms(source, meter)?;
    let converter = Converter { options };
    let mut values = Vec::new();
    for (value, _) in converter.items(forms, 0)? {
        values.push(value);
    }
    Ok(values)
}

/// Read exactly one EDN value; nothing but whitespace and comments may follow.
pub fn read_one(source: &str, options: &Options) -> Result<Value, Error> {
    let mut values = read_all(source, options)?;
    match values.len() {
        1 => Ok(values.pop().unwrap()),
        0 => Err(Error {
            message: "Expected an EDN value".into(),
            span: Some(source.len()..source.len()),
        }),
        _ => Err(Error {
            message: "Expected exactly one EDN value".into(),
            span: Some(0..source.len()),
        }),
    }
}

struct Converter<'o> {
    options: &'o Options,
}
fn failure(span: Range<usize>, message: impl Into<String>) -> Error {
    Error {
        message: message.into(),
        span: Some(span),
    }
}
impl Converter<'_> {
    fn value(&self, form: Form, depth: usize) -> Result<Value, Error> {
        if depth > self.options.max_depth {
            return Err(failure(form.span.clone(), "EDN nesting exceeds the depth limit"));
        }
        stacker::maybe_grow(64 * 1024, 1024 * 1024, || self.value_inner(form, depth))
    }
    fn value_inner(&self, mut form: Form, depth: usize) -> Result<Value, Error> {
        let span = form.span.clone();
        let metadata = std::mem::take(&mut form.metadata);
        let value = match form.take_kind() {
            Kind::Nil => Value::Nil,
            Kind::Bool(b) => Value::Bool(b),
            Kind::Number(x) => Value::Number(x),
            Kind::String(units) => Value::String(
                String::from_utf16(&units)
                    .map_err(|_| failure(span.clone(), "EDN strings must be valid Unicode"))?,
            ),
            Kind::Symbol(symbol) => Value::Symbol(symbol),
            Kind::Keyword(keyword) => Value::Keyword(keyword),
            Kind::List(items) => Value::List(self.plain(items, depth)?),
            Kind::Vector(items) => Value::Vector(self.plain(items, depth)?),
            Kind::Set(items) => {
                let mut set = BTreeSet::new();
                for (item, item_span) in self.items(items, depth + 1)? {
                    if let Some(item) = set.replace(item) {
                        return Err(duplicate(item_span, &item));
                    }
                }
                Value::Set(set)
            }
            Kind::Map(items) => {
                let items = self.items(items, depth + 1)?;
                if items.len() % 2 != 0 {
                    return Err(failure(span, "Map requires an even number of forms"));
                }
                let mut map = BTreeMap::new();
                let mut items = items.into_iter();
                while let (Some((key, key_span)), Some((value, _))) = (items.next(), items.next()) {
                    if map.contains_key(&key) {
                        return Err(duplicate(key_span, &key));
                    }
                    map.insert(key, value);
                }
                Value::Map(map)
            }
            Kind::Tagged { tag, form } => {
                let value = self.value(*form, depth + 1)?;
                self.tagged(&tag, value).map_err(|message| failure(span.clone(), message))?
            }
            Kind::Discard(_)
            | Kind::Prefix { .. }
            | Kind::Conditional(_)
            | Kind::SplicingConditional(_) => {
                return Err(failure(span, "Unsupported EDN syntax"));
            }
        };
        if metadata.is_empty() {
            return Ok(value);
        }
        // The outermost `^` is first and wins, as Clojure's reader merges it last.
        let mut meta = BTreeMap::new();
        for form in metadata.into_iter().rev() {
            let meta_span = form.span.clone();
            match self.value(form, depth + 1)? {
                Value::Map(entries) => meta.extend(entries),
                Value::Keyword(keyword) => {
                    meta.insert(Value::Keyword(keyword), Value::Bool(true));
                }
                tag @ (Value::Symbol(_) | Value::String(_)) => {
                    meta.insert(Value::keyword("tag"), tag);
                }
                _ => {
                    return Err(failure(
                        meta_span,
                        "Metadata must be a keyword, symbol, string or map",
                    ));
                }
            }
        }
        Ok(if meta.is_empty() {
            value
        } else {
            Value::Meta(Box::new(value), meta)
        })
    }
    fn plain(&self, items: Vec<Form>, depth: usize) -> Result<Vec<Value>, Error> {
        Ok(self.items(items, depth + 1)?.into_iter().map(|(value, _)| value).collect())
    }
    /// Items with their spans. A discarded form is still read, so its tags
    /// are checked, as `clojure.edn` does.
    fn items(&self, items: Vec<Form>, depth: usize) -> Result<Vec<(Value, Range<usize>)>, Error> {
        let mut values = Vec::with_capacity(items.len());
        for mut item in items {
            let span = item.span.clone();
            if matches!(item.kind, Kind::Discard(_)) {
                let Kind::Discard(target) = item.take_kind() else { unreachable!() };
                self.value(*target, depth)?;
                continue;
            }
            values.push((self.value(item, depth)?, span));
        }
        Ok(values)
    }
    fn tagged(&self, tag: &Symbol, value: Value) -> Result<Value, String> {
        if let Some(reader) = self.options.readers.get(tag) {
            return reader(value);
        }
        if tag.namespace.is_none() && tag.name == "queue" {
            return match value.unmeta() {
                Value::Vector(_) => Ok(Value::Tagged(tag.clone(), Box::new(value))),
                _ => Err("Queue literal expects a vector for its elements.".into()),
            };
        }
        match &self.options.default {
            Some(default) => default(tag, value),
            None => Err(format!("No reader function for tag {tag}")),
        }
    }
}
fn duplicate(span: Range<usize>, key: &Value) -> Error {
    let printed = print(key).unwrap_or_else(|_| format!("{key:?}"));
    failure(span, format!("Duplicate key: {printed}"))
}
