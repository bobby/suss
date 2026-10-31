//! Canonical EDN for tooling data (#299): a value model, a deterministic
//! printer and the `suss.edn` reading contract, following `clojure.edn`.
//!
//! Reading parses EDN directly, with the forms reader's lexical rules and
//! messages. Code-only syntax (quote sugar, syntax quote, `#()`, `#'`, reader
//! conditionals) is an error at its span. Tags are an error unless built in
//! (`#queue`) or handled by [`Options::readers`] or [`Options::default`].
//! Input size is bounded by [`Options::work_allowance`] and nesting by
//! [`Options::max_depth`].
//!
//! Printing is canonical: map and set entries in [`Value`]'s total order,
//! shortest round-trip numbers and one string escaping, so equal values print
//! byte-identically in every process.
use crate::forms;
use crate::{Keyword, ParseError, Symbol};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet, HashMap, btree_map},
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
        let mut value = self;
        while let Value::Meta(inner, _) = value {
            value = inner;
        }
        value
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
        match (self.unmeta(), other.unmeta()) {
            (Value::Keyword(a), Value::Keyword(b)) => {
                (&a.namespace, &a.name).cmp(&(&b.namespace, &b.name))
            }
            (Value::String(a), Value::String(b)) => a.cmp(b),
            (Value::Number(a), Value::Number(b)) => number_cmp(*a, *b),
            // Only collections recurse, so only they need a growable stack.
            (a, b) => stacker::maybe_grow(64 * 1024, 1024 * 1024, || a.cmp_slow(b)),
        }
    }
}
impl Value {
    fn cmp_slow(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Value::Bool(a), Value::Bool(b)) => a.cmp(b),
            (Value::Number(a), Value::Number(b)) => number_cmp(*a, *b),
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
        }
    }
}
/// Every NaN is one value, as `##NaN` prints and reads; otherwise total order.
fn number_cmp(a: f64, b: f64) -> Ordering {
    match (a.is_nan(), b.is_nan()) {
        (true, true) => Ordering::Equal,
        _ => a.total_cmp(&b),
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
                out.push_str(&tag_text(tag, value)?);
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
                let tag = tag_text(tag, value)?;
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
fn tag_text(tag: &Symbol, value: &Value) -> Result<String, Error> {
    let text = symbol_text(tag)?;
    if !text.starts_with(char::is_alphabetic) {
        return Err(Error::new(format!("Tag {text:?} must start with a letter")));
    }
    // The built-in tag reads only a vector back.
    if text == "queue" && !matches!(value.unmeta(), Value::Vector(_)) {
        return Err(Error::new("Queue literal expects a vector for its elements."));
    }
    Ok(text)
}

// ---------------------------------------------------------------------------
// Reading

/// A reader for one tag: its value in, the tagged value out.
pub type TagReader = dyn Fn(Value) -> Result<Value, String>;
/// The fallback for tags with no reader, as `clojure.edn`'s `:default`.
pub type DefaultReader = dyn Fn(&Symbol, Value) -> Result<Value, String>;

/// Reading options, following `clojure.edn`'s `:readers` and `:default`.
pub struct Options {
    /// Reader work units: one per source byte and one per form.
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
    let mut parser = Parser::new(source, options)?;
    let mut values = Vec::new();
    loop {
        parser.padding();
        if parser.pos == source.len() {
            return Ok(values);
        }
        if let Some((value, _)) = parser.form(0)? {
            values.push(value);
        }
    }
}

/// Read exactly one EDN value; nothing but whitespace and comments may follow.
pub fn read_one(source: &str, options: &Options) -> Result<Value, Error> {
    let mut values = read_all(source, options)?;
    match values.len() {
        1 => Ok(values.pop().unwrap()),
        0 => Err(failure(source.len()..source.len(), "Expected an EDN value")),
        _ => Err(failure(0..source.len(), "Expected exactly one EDN value")),
    }
}

fn failure(span: Range<usize>, message: impl Into<String>) -> Error {
    Error {
        message: message.into(),
        span: Some(span),
    }
}
fn duplicate(span: Range<usize>, key: &Value) -> Error {
    let printed = print(key).unwrap_or_else(|_| format!("{key:?}"));
    failure(span, format!("Duplicate key: {printed}"))
}

/// A direct EDN parser over the source bytes. Its lexical rules, messages and
/// spans are the forms reader's (`crate::forms`), whose number, symbol and
/// delimiter functions it shares; it builds values without intermediate forms.
struct Parser<'s, 'o> {
    source: &'s str,
    bytes: &'s [u8],
    pos: usize,
    options: &'o Options,
    work: u64,
}
type Item = (Value, Range<usize>);

impl<'s> Parser<'s, '_> {
    fn new<'o>(source: &'s str, options: &'o Options) -> Result<Parser<'s, 'o>, Error> {
        // One unit per byte, charged up front so size is bounded before reading.
        let work = options
            .work_allowance
            .checked_sub(source.len() as u64)
            .ok_or_else(|| failure(0..source.len(), "Reader work budget exhausted"))?;
        Ok(Parser {
            source,
            bytes: source.as_bytes(),
            pos: 0,
            options,
            work,
        })
    }
    fn peek_char(&self) -> Option<char> {
        self.source[self.pos..].chars().next()
    }
    fn take_char(&mut self) -> Option<char> {
        let c = self.peek_char()?;
        self.pos += c.len_utf8();
        Some(c)
    }
    fn padding(&mut self) {
        while let Some(&b) = self.bytes.get(self.pos) {
            if b == b',' || (b < 0x80 && (b as char).is_whitespace()) {
                self.pos += 1;
            } else if b == b';' {
                while let Some(c) = self.take_char() {
                    if c == '\n' {
                        break;
                    }
                }
            } else if b >= 0x80 && self.peek_char().is_some_and(char::is_whitespace) {
                self.take_char();
            } else {
                return;
            }
        }
    }
    fn token(&mut self) -> &'s str {
        let source = self.source;
        let start = self.pos;
        while let Some(&b) = self.bytes.get(self.pos) {
            if b < 0x80 {
                if forms::delimiter(b as char) {
                    break;
                }
                self.pos += 1;
            } else {
                let c = self.peek_char().unwrap();
                if forms::delimiter(c) {
                    break;
                }
                self.pos += c.len_utf8();
            }
        }
        &source[start..self.pos]
    }
    /// The next value that is not discarded.
    fn required(&mut self, depth: usize) -> Result<Item, Error> {
        loop {
            self.padding();
            if self.pos == self.source.len() {
                return Err(failure(self.pos..self.pos, "Expected a form before end of input"));
            }
            if let Some(item) = self.form(depth)? {
                return Ok(item);
            }
        }
    }
    /// Collection items, with their spans only when `spans` is given.
    fn items(
        &mut self,
        start: usize,
        close: u8,
        depth: usize,
        mut spans: Option<&mut Vec<Range<usize>>>,
    ) -> Result<Vec<Value>, Error> {
        let mut items = Vec::new();
        loop {
            self.padding();
            match self.bytes.get(self.pos) {
                None => {
                    return Err(failure(
                        start..self.pos,
                        format!("Unclosed collection; expected {}", close as char),
                    ));
                }
                Some(&b) if b == close => {
                    self.pos += 1;
                    return Ok(items);
                }
                _ => {
                    if let Some((item, span)) = self.form(depth)? {
                        items.push(item);
                        if let Some(spans) = spans.as_mut() {
                            spans.push(span);
                        }
                    }
                }
            }
        }
    }
    /// One form at the current, non-padding position; `None` for a discard.
    fn form(&mut self, depth: usize) -> Result<Option<Item>, Error> {
        let start = self.pos;
        self.work = self
            .work
            .checked_sub(1)
            .ok_or_else(|| failure(start..start, "Reader work budget exhausted"))?;
        if depth > self.options.max_depth {
            return Err(failure(start..start + 1, "EDN nesting exceeds the depth limit"));
        }
        let value = stacker::maybe_grow(64 * 1024, 1024 * 1024, || self.form_inner(start, depth))?;
        Ok(value.map(|value| (value, start..self.pos)))
    }
    fn form_inner(&mut self, start: usize, depth: usize) -> Result<Option<Value>, Error> {
        let fail = |parser: &Self, message: &str| Err(failure(start..parser.pos, message));
        let b = self.bytes[start];
        if b < 0x80 && !matches!(b, b'(' | b'[' | b'{' | b')' | b']' | b'}' | b'"' | b'\\' | b'^' | b'\'' | b'`' | b'~' | b'@' | b'#') {
            return self.atom(start).map(Some);
        }
        if b >= 0x80 {
            return self.atom(start).map(Some);
        }
        self.pos += 1;
        Ok(Some(match b {
            b'(' => Value::List(self.items(start, b')', depth + 1, None)?),
            b'[' => Value::Vector(self.items(start, b']', depth + 1, None)?),
            b'{' => {
                let mut spans = Vec::new();
                let items = self.items(start, b'}', depth + 1, Some(&mut spans))?;
                if items.len() % 2 != 0 {
                    return fail(self, "Map requires an even number of forms");
                }
                let mut map = BTreeMap::new();
                let mut items = items.into_iter();
                let mut spans = spans.into_iter().step_by(2);
                while let (Some(key), Some(value), Some(key_span)) = (items.next(), items.next(), spans.next()) {
                    match map.entry(key) {
                        btree_map::Entry::Vacant(entry) => {
                            entry.insert(value);
                        }
                        btree_map::Entry::Occupied(entry) => {
                            return Err(duplicate(key_span, entry.key()));
                        }
                    }
                }
                Value::Map(map)
            }
            b')' | b']' | b'}' => return fail(self, "Unexpected closing delimiter"),
            b'"' => Value::String(self.string(start)?),
            b'\\' => Value::String(self.character(start)?),
            b'^' => self.metadata(depth)?,
            b'#' => return self.dispatch(start, depth),
            _ => return fail(self, "Quote, syntax-quote, unquote and deref syntax are not EDN"),
        }))
    }
    fn atom(&mut self, start: usize) -> Result<Value, Error> {
        let token = self.token();
        let fail = |parser: &Self, message: &str| failure(start..parser.pos, message);
        Ok(match token {
            "nil" => Value::Nil,
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            token if token.starts_with(':') => {
                if token.starts_with("::") {
                    return Err(fail(self, "Auto-resolved keywords require namespace resolution"));
                }
                if !forms::valid_symbol(&token[1..]) {
                    return Err(fail(self, "Invalid keyword"));
                }
                Value::Keyword(Keyword::parse(&token[1..]))
            }
            token if forms::numeric_start(token) => {
                Value::Number(forms::number(token).map_err(|message| fail(self, message))?)
            }
            token => {
                if !forms::valid_symbol(token) {
                    return Err(fail(self, "Invalid symbol"));
                }
                Value::Symbol(Symbol::parse(token))
            }
        })
    }
    fn dispatch(&mut self, start: usize, depth: usize) -> Result<Option<Value>, Error> {
        let fail = |parser: &Self, message: &str| Err(failure(start..parser.pos, message));
        match self.peek_char() {
            Some('{') => {
                self.pos += 1;
                let mut set = BTreeSet::new();
                let mut spans = Vec::new();
                let items = self.items(start, b'}', depth + 1, Some(&mut spans))?;
                for (item, span) in items.into_iter().zip(spans) {
                    if let Some(item) = set.replace(item) {
                        return Err(duplicate(span, &item));
                    }
                }
                Ok(Some(Value::Set(set)))
            }
            Some('_') => {
                // Still read, so its tags are checked, as `clojure.edn` does.
                self.pos += 1;
                self.required(depth + 1)?;
                Ok(None)
            }
            Some('#') => {
                self.pos += 1;
                Ok(Some(Value::Number(match self.token() {
                    "Inf" => f64::INFINITY,
                    "-Inf" => f64::NEG_INFINITY,
                    "NaN" => f64::NAN,
                    _ => return fail(self, "Unknown symbolic numeric literal"),
                })))
            }
            Some(c @ ('(' | '\'' | '?')) => {
                self.pos += 1;
                fail(self, match c {
                    '(' => "Anonymous function syntax is not EDN",
                    '\'' => "Var syntax is not EDN",
                    _ => "Reader conditionals are not EDN",
                })
            }
            Some(c) if c.is_alphabetic() => {
                let token = self.token();
                if matches!(token, "nil" | "true" | "false") {
                    return fail(self, "Reader tag must be a symbol");
                }
                if !forms::valid_symbol(token) {
                    return fail(self, "Invalid tag");
                }
                let tag = Symbol::parse(token);
                let (value, _) = self.required(depth + 1)?;
                let value = self.tagged(&tag, value).map_err(|message| failure(start..self.pos, message))?;
                Ok(Some(value))
            }
            _ => {
                self.take_char();
                fail(self, "Unsupported reader dispatch")
            }
        }
    }
    fn metadata(&mut self, depth: usize) -> Result<Value, Error> {
        let (meta, meta_span) = self.required(depth + 1)?;
        let entries = match meta {
            // Metadata on the metadata map is dropped, as Clojure's reader does.
            Value::Map(entries) => entries,
            Value::Meta(map, _) if matches!(*map, Value::Map(_)) => {
                let Value::Map(entries) = *map else { unreachable!() };
                entries
            }
            Value::Keyword(keyword) => BTreeMap::from([(Value::Keyword(keyword), Value::Bool(true))]),
            tag @ (Value::Symbol(_) | Value::String(_)) => BTreeMap::from([(Value::keyword("tag"), tag)]),
            _ => return Err(failure(meta_span, "Metadata must be a keyword, symbol, string or map")),
        };
        let (target, target_span) = self.required(depth + 1)?;
        if check_meta_target(target.unmeta()).is_err() {
            return Err(failure(target_span, "Metadata requires a symbol or collection"));
        }
        // This, the outer `^`, wins over the target's own metadata, as
        // Clojure's reader merges it last.
        Ok(match target {
            Value::Meta(value, mut existing) => {
                existing.extend(entries);
                Value::Meta(value, existing)
            }
            target if entries.is_empty() => target,
            target => Value::Meta(Box::new(target), entries),
        })
    }
    fn string(&mut self, start: usize) -> Result<String, Error> {
        let mut out = String::new();
        let mut high: Option<u16> = None;
        let mut valid = true;
        loop {
            let run = self.pos;
            while let Some(&b) = self.bytes.get(self.pos) {
                if b == b'"' || b == b'\\' {
                    break;
                }
                self.pos += 1;
            }
            if self.pos > run {
                valid &= high.take().is_none();
                out.push_str(&self.source[run..self.pos]);
            }
            match self.bytes.get(self.pos) {
                None => return Err(failure(start..self.pos, "Unclosed string")),
                Some(b'"') => {
                    self.pos += 1;
                    if !valid || high.is_some() {
                        return Err(failure(start..self.pos, "EDN strings must be valid Unicode"));
                    }
                    return Ok(out);
                }
                Some(_) => {
                    let unit = self.escape()?;
                    match (high.take(), unit) {
                        (None, 0xd800..=0xdbff) => high = Some(unit),
                        (Some(first), 0xdc00..=0xdfff) => {
                            let scalar = 0x10000 + ((u32::from(first) - 0xd800) << 10) + (u32::from(unit) - 0xdc00);
                            out.push(char::from_u32(scalar).unwrap());
                        }
                        (Some(_), _) | (None, 0xdc00..=0xdfff) => valid = false,
                        (None, unit) => out.push(char::from_u32(unit.into()).unwrap()),
                    }
                }
            }
        }
    }
    /// One escape after its backslash, as the forms reader decodes it.
    fn escape(&mut self) -> Result<u16, Error> {
        let escape_start = self.pos;
        self.pos += 1;
        let fail = |parser: &Self, message: &str| failure(escape_start..parser.pos, message);
        Ok(match self.take_char() {
            Some('n') => 10,
            Some('r') => 13,
            Some('t') => 9,
            Some('b') => 8,
            Some('f') => 12,
            Some('"') => 34,
            Some('\\') => 92,
            Some('u') => {
                let mut unit = 0;
                for _ in 0..4 {
                    let digit = self
                        .take_char()
                        .and_then(|c| c.to_digit(16))
                        .ok_or_else(|| fail(self, "Unicode escape requires four hexadecimal digits"))?;
                    unit = unit * 16 + digit as u16;
                }
                unit
            }
            Some(c @ '0'..='7') => {
                let mut unit = c as u16 - '0' as u16;
                for _ in 0..2 {
                    match self.peek_char() {
                        Some(c @ '0'..='7') => {
                            self.pos += 1;
                            unit = unit * 8 + c as u16 - '0' as u16;
                        }
                        Some(c) if !forms::delimiter(c) && !matches!(c, '#' | '\'') => {
                            self.take_char();
                            return Err(fail(self, "Invalid octal escape digit"));
                        }
                        _ => break,
                    }
                }
                if unit > 255 {
                    return Err(fail(self, "Octal escape exceeds one byte"));
                }
                unit
            }
            _ => return Err(fail(self, "Invalid string escape")),
        })
    }
    fn character(&mut self, start: usize) -> Result<String, Error> {
        let fail = |parser: &Self, message: &str| failure(start..parser.pos, message);
        let first = self.take_char().ok_or_else(|| fail(self, "Missing character literal"))?;
        if forms::delimiter(first) {
            return Ok(first.into());
        }
        let token_start = self.pos - first.len_utf8();
        self.token();
        let token = &self.source[token_start..self.pos];
        let unit = match token {
            "newline" => 10,
            "return" => 13,
            "tab" => 9,
            "space" => 32,
            "backspace" => 8,
            "formfeed" => 12,
            _ if token.starts_with('u') && token.len() == 5 => u16::from_str_radix(&token[1..], 16)
                .map_err(|_| fail(self, "Invalid Unicode character literal"))?,
            _ if token.starts_with('o') && (2..=4).contains(&token.len()) => {
                let unit = u16::from_str_radix(&token[1..], 8)
                    .map_err(|_| fail(self, "Invalid octal character literal"))?;
                if unit > 255 {
                    return Err(fail(self, "Octal character exceeds one byte"));
                }
                unit
            }
            _ if token.encode_utf16().count() == 1 => token.encode_utf16().next().unwrap(),
            _ => return Err(fail(self, "Character literal requires one UTF-16 unit")),
        };
        match char::from_u32(unit.into()) {
            Some(c) => Ok(c.into()),
            None => Err(fail(self, "Surrogate character literal is unsupported; use a string")),
        }
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
