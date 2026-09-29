//! Portable source forms, separate from prototype EDN runtime values.
//!
//! Byte spans refer to the original UTF-8 source. String contents are UTF-16,
//! including lone surrogates; numbers are ordinary binary64, never JVM ratios.
//! Reader metadata remains syntax until phase analysis interprets it. Nesting is
//! bounded to 64 forms to keep diagnostics safe on ordinary thread stacks.
use crate::{Keyword, ParseError, Symbol};
use std::ops::Range;

#[derive(Debug, Clone, PartialEq)]
pub struct Form {
    pub span: Range<usize>,
    /// Prefix metadata in source order, with each metadata form's own span.
    pub metadata: Vec<Form>,
    pub kind: Kind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    Nil,
    Bool(bool),
    Number(f64),
    String(Vec<u16>),
    Symbol(Symbol),
    Keyword(Keyword),
    List(Vec<Form>),
    Vector(Vec<Form>),
    Set(Vec<Form>),
    /// Ordered syntax entries; pairing is checked after conditional removal.
    Map(Vec<Form>),
    Conditional(Vec<(Keyword, Form)>),
}

/// Read portable syntax without namespace resolution or macro execution.
/// Unsupported dispatch/numeric syntax fails explicitly, including ratios and
/// arbitrary-precision suffixes. Empty/comment-only sources yield no forms.
pub fn read_forms(source: &str) -> Result<Vec<Form>, ParseError> {
    let mut reader = Reader {
        source,
        offset: 0,
        depth: 0,
    };
    let mut forms = Vec::new();
    loop {
        reader.padding();
        if reader.peek().is_none() {
            return Ok(forms);
        }
        if let Some(form) = reader.form()? {
            forms.push(form);
        }
    }
}

/// Resolve portable :suss/:cljs conditionals, taking the first matching clause
/// in source order. Unmatched forms disappear. :default matches where written.
/// Syntax quoting, namespace aliases and phase imports belong to later phases.
pub fn resolve_conditionals(forms: Vec<Form>) -> Result<Vec<Form>, ParseError> {
    forms
        .into_iter()
        .filter_map(|form| match select(form) {
            Ok(Some(form)) => Some(Ok(form)),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        })
        .collect()
}

fn select(mut form: Form) -> Result<Option<Form>, ParseError> {
    let select_items = |items: Vec<Form>| resolve_conditionals(items);
    form.kind = match form.kind {
        Kind::Conditional(clauses) => {
            for (key, mut value) in clauses {
                if key.namespace.is_none()
                    && matches!(key.name.as_str(), "suss" | "cljs" | "default")
                {
                    // Prefix metadata remains attached to the selected syntax.
                    value.metadata.splice(0..0, form.metadata);
                    return select(value);
                }
            }
            return Ok(None);
        }
        Kind::List(items) => Kind::List(select_items(items)?),
        Kind::Vector(items) => Kind::Vector(select_items(items)?),
        Kind::Set(items) => Kind::Set(select_items(items)?),
        Kind::Map(items) => {
            let items = select_items(items)?;
            if items.len() % 2 != 0 {
                return Err(error(
                    form.span.clone(),
                    "Conditional leaves an incomplete map entry",
                ));
            }
            Kind::Map(items)
        }
        other => other,
    };
    form.metadata = resolve_conditionals(form.metadata)?;
    Ok(Some(form))
}

fn error(span: Range<usize>, message: impl Into<String>) -> ParseError {
    ParseError {
        message: message.into(),
        span,
        expected: Vec::new(),
    }
}
fn delimiter(c: char) -> bool {
    c.is_whitespace()
        || matches!(
            c,
            ',' | '('
                | ')'
                | '['
                | ']'
                | '{'
                | '}'
                | '"'
                | ';'
                | '\''
                | '`'
                | '~'
                | '@'
                | '^'
                | '\\'
        )
}

struct Reader<'a> {
    source: &'a str,
    offset: usize,
    depth: usize,
}
impl Reader<'_> {
    fn peek(&self) -> Option<char> {
        self.source[self.offset..].chars().next()
    }
    fn take(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.offset += c.len_utf8();
        Some(c)
    }
    fn padding(&mut self) {
        loop {
            while self.peek().is_some_and(|c| c.is_whitespace() || c == ',') {
                self.take();
            }
            if self.peek() != Some(';') {
                break;
            }
            while self.take().is_some_and(|c| c != '\n') {}
        }
    }
    fn fail(&self, start: usize, message: impl Into<String>) -> ParseError {
        error(start..self.offset, message)
    }
    fn token(&mut self) -> &str {
        let start = self.offset;
        while self.peek().is_some_and(|c| !delimiter(c)) {
            self.take();
        }
        &self.source[start..self.offset]
    }
    fn required(&mut self) -> Result<Form, ParseError> {
        loop {
            self.padding();
            if self.peek().is_none() {
                return Err(self.fail(self.offset, "Expected a form before end of input"));
            }
            if let Some(form) = self.form()? {
                return Ok(form);
            }
        }
    }
    fn form(&mut self) -> Result<Option<Form>, ParseError> {
        self.padding();
        if self.depth >= 64 {
            return Err(self.fail(self.offset, "Reader nesting limit exceeded"));
        }
        self.depth += 1;
        let result = self.form_inner();
        self.depth -= 1;
        result
    }
    fn form_inner(&mut self) -> Result<Option<Form>, ParseError> {
        let start = self.offset;
        let Some(c) = self.take() else {
            return Err(self.fail(start, "Expected a form"));
        };
        let kind = match c {
            '(' => Kind::List(self.items(start, ')')?),
            '[' => Kind::Vector(self.items(start, ']')?),
            '{' => {
                let items = self.items(start, '}')?;
                if items.len() % 2 != 0
                    && !items
                        .iter()
                        .any(|item| matches!(item.kind, Kind::Conditional(_)))
                {
                    return Err(self.fail(start, "Map requires an even number of forms"));
                }
                Kind::Map(items)
            }
            ')' | ']' | '}' => return Err(self.fail(start, "Unexpected closing delimiter")),
            '"' => Kind::String(self.string(start)?),
            '\\' => Kind::String(vec![self.character(start)?]),
            '^' => {
                let metadata = self.required()?;
                if !matches!(
                    metadata.kind,
                    Kind::Keyword(_) | Kind::Symbol(_) | Kind::String(_) | Kind::Map(_)
                ) {
                    return Err(error(
                        metadata.span,
                        "Metadata must be a keyword, symbol, string or map",
                    ));
                }
                let mut target = self.required()?;
                if !matches!(
                    target.kind,
                    Kind::Symbol(_) | Kind::List(_) | Kind::Vector(_) | Kind::Map(_) | Kind::Set(_)
                ) {
                    return Err(error(
                        target.span,
                        "Metadata requires a symbol or collection",
                    ));
                }
                target.metadata.insert(0, metadata);
                target.span.start = start;
                return Ok(Some(target));
            }
            '\'' | '`' | '~' | '@' => {
                let name = match c {
                    '\'' => "quote",
                    '`' => "syntax-quote",
                    '@' => "deref",
                    '~' if self.peek() == Some('@') => {
                        self.take();
                        "unquote-splicing"
                    }
                    _ => "unquote",
                };
                self.wrapper(start, name)?
            }
            '#' => match self.take() {
                Some('{') => Kind::Set(self.items(start, '}')?),
                Some('_') => {
                    self.required()?;
                    return Ok(None);
                }
                Some('\'') => self.wrapper(start, "var")?,
                Some('?') => {
                    if self.peek() == Some('@') {
                        self.take();
                        return Err(
                            self.fail(start, "Splicing reader conditionals are not implemented")
                        );
                    }
                    self.padding();
                    if self.take() != Some('(') {
                        return Err(self.fail(start, "Reader conditional requires a list"));
                    }
                    let items = self.items(start, ')')?;
                    if items.len() % 2 != 0 {
                        return Err(
                            self.fail(start, "Reader conditional requires feature/form pairs")
                        );
                    }
                    let mut items = items.into_iter();
                    let mut clauses = Vec::new();
                    while let Some(key) = items.next() {
                        let Kind::Keyword(key_kind) = key.kind else {
                            return Err(error(key.span, "Reader feature must be a keyword"));
                        };
                        clauses.push((key_kind, items.next().unwrap()));
                    }
                    Kind::Conditional(clauses)
                }
                Some('#') => {
                    let token = self.token();
                    Kind::Number(match token {
                        "Inf" => f64::INFINITY,
                        "-Inf" => f64::NEG_INFINITY,
                        "NaN" => f64::NAN,
                        _ => return Err(self.fail(start, "Unknown symbolic numeric literal")),
                    })
                }
                _ => return Err(self.fail(start, "Unsupported reader dispatch")),
            },
            _ => {
                self.offset = start;
                let token = self.token();
                match token {
                    "nil" => Kind::Nil,
                    "true" => Kind::Bool(true),
                    "false" => Kind::Bool(false),
                    token if token.starts_with(':') => {
                        if token.starts_with("::") {
                            return Err(self.fail(
                                start,
                                "Auto-resolved keywords require namespace resolution",
                            ));
                        }
                        if !valid_symbol(&token[1..]) {
                            return Err(self.fail(start, "Invalid keyword"));
                        }
                        Kind::Keyword(Keyword::parse(&token[1..]))
                    }
                    token if numeric_start(token) => {
                        Kind::Number(number(token).map_err(|message| self.fail(start, message))?)
                    }
                    token => {
                        if !valid_symbol(token) {
                            return Err(self.fail(start, "Invalid symbol"));
                        }
                        Kind::Symbol(Symbol::parse(token))
                    }
                }
            }
        };
        Ok(Some(Form {
            span: start..self.offset,
            metadata: Vec::new(),
            kind,
        }))
    }
    fn wrapper(&mut self, start: usize, name: &str) -> Result<Kind, ParseError> {
        let prefix = Form {
            span: start..self.offset,
            metadata: Vec::new(),
            kind: Kind::Symbol(Symbol::new(name)),
        };
        Ok(Kind::List(vec![prefix, self.required()?]))
    }
    fn items(&mut self, start: usize, end: char) -> Result<Vec<Form>, ParseError> {
        let mut items = Vec::new();
        loop {
            self.padding();
            match self.peek() {
                Some(c) if c == end => {
                    self.take();
                    return Ok(items);
                }
                None => {
                    return Err(self.fail(start, format!("Unclosed collection; expected {end}")));
                }
                _ => {
                    if let Some(form) = self.form()? {
                        items.push(form);
                    }
                }
            }
        }
    }
    fn string(&mut self, start: usize) -> Result<Vec<u16>, ParseError> {
        let mut units = Vec::new();
        loop {
            let escape_start = self.offset;
            match self.take() {
                None => return Err(self.fail(start, "Unclosed string")),
                Some('"') => return Ok(units),
                Some('\\') => {
                    let unit = match self.take() {
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
                                let digit =
                                    self.take().and_then(|c| c.to_digit(16)).ok_or_else(|| {
                                        self.fail(
                                            escape_start,
                                            "Unicode escape requires four hexadecimal digits",
                                        )
                                    })?;
                                unit = unit * 16 + digit as u16;
                            }
                            unit
                        }
                        Some(c @ '0'..='7') => {
                            let mut unit = c as u16 - '0' as u16;
                            for _ in 0..2 {
                                match self.peek() {
                                    Some(c @ '0'..='7') => {
                                        self.take();
                                        unit = unit * 8 + c as u16 - '0' as u16;
                                    }
                                    Some(c) if !delimiter(c) && c != '#' => {
                                        self.take();
                                        return Err(
                                            self.fail(escape_start, "Invalid octal escape digit")
                                        );
                                    }
                                    _ => break,
                                }
                            }
                            if unit > 255 {
                                return Err(
                                    self.fail(escape_start, "Octal escape exceeds one byte")
                                );
                            }
                            unit
                        }
                        _ => return Err(self.fail(escape_start, "Invalid string escape")),
                    };
                    units.push(unit);
                }
                Some(c) => {
                    let mut buffer = [0; 2];
                    units.extend_from_slice(c.encode_utf16(&mut buffer));
                }
            }
        }
    }
    fn character(&mut self, start: usize) -> Result<u16, ParseError> {
        let first = self
            .take()
            .ok_or_else(|| self.fail(start, "Missing character literal"))?;
        if delimiter(first) {
            return Ok(first as u16);
        }
        let token_start = self.offset - first.len_utf8();
        self.token();
        let token = &self.source[token_start..self.offset];
        let unit = match token {
            "newline" => 10,
            "return" => 13,
            "tab" => 9,
            "space" => 32,
            "backspace" => 8,
            "formfeed" => 12,
            _ if token.starts_with('u') && token.len() == 5 => u16::from_str_radix(&token[1..], 16)
                .map_err(|_| self.fail(start, "Invalid Unicode character literal"))?,
            _ if token.starts_with('o') && (2..=4).contains(&token.len()) => {
                let unit = u16::from_str_radix(&token[1..], 8)
                    .map_err(|_| self.fail(start, "Invalid octal character literal"))?;
                if unit > 255 {
                    return Err(self.fail(start, "Octal character exceeds one byte"));
                }
                unit
            }
            _ if token.encode_utf16().count() == 1 => token.encode_utf16().next().unwrap(),
            _ => return Err(self.fail(start, "Character literal requires one UTF-16 unit")),
        };
        if (0xd800..=0xdfff).contains(&unit) {
            return Err(self.fail(
                start,
                "Surrogate character literal is unsupported; use a string",
            ));
        }
        Ok(unit)
    }
}

fn numeric_start(token: &str) -> bool {
    let mut chars = token.chars();
    match chars.next() {
        Some('0'..='9') => true,
        Some('+' | '-') => chars.next().is_some_and(|c| c.is_ascii_digit()),
        _ => false,
    }
}
fn number(token: &str) -> Result<f64, &'static str> {
    if token.contains('/') || token.ends_with(['N', 'M']) {
        return Err(
            "Unsupported numeric form: ratios and precision suffixes are not portable ordinary numbers",
        );
    }
    let negative = token.starts_with('-');
    let digits = token.strip_prefix(['+', '-']).unwrap_or(token);
    let radix = if let Some(hex) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        Some((16, hex))
    } else if let Some((base, digits)) = digits.split_once(['r', 'R']) {
        let base: u32 = base.parse().map_err(|_| "Invalid numeric radix")?;
        if !(2..=36).contains(&base) {
            return Err("Numeric radix must be between 2 and 36");
        }
        Some((base, digits))
    } else if digits.len() > 1
        && digits.starts_with('0')
        && digits.chars().all(|c| c.is_ascii_digit())
    {
        Some((8, &digits[1..]))
    } else {
        None
    };
    if let Some((base, digits)) = radix {
        if digits.is_empty()
            || !digits
                .chars()
                .all(|c| c.is_ascii() && c.to_digit(base).is_some())
        {
            return Err("Invalid numeric digits");
        }
        let integer = num_bigint::BigInt::parse_bytes(digits.as_bytes(), base)
            .ok_or("Invalid numeric digits")?;
        let value = crate::Number::Integer(integer).to_f64();
        return Ok(if negative { -value } else { value });
    }
    if !digits
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '+' | '-'))
    {
        return Err("Invalid ordinary numeric literal");
    }
    let value: f64 = token
        .parse()
        .map_err(|_| "Invalid ordinary numeric literal")?;
    // The pinned ClojureScript integer reader spells either signed integer zero
    // as +0. Floating syntax, including -0.0/-0e0, retains the sign.
    if value == 0.0 && !digits.contains(['.', 'e', 'E']) {
        Ok(0.0)
    } else {
        Ok(value)
    }
}
fn valid_symbol(token: &str) -> bool {
    if token.is_empty() || token.ends_with(':') || token.starts_with(':') {
        return false;
    }
    if token == "/" {
        return true;
    }
    match token.split_once('/') {
        None => true,
        Some((namespace, name)) => {
            !namespace.is_empty()
                && !namespace.ends_with(':')
                && !name.is_empty()
                && !name.starts_with(|c: char| c.is_ascii_digit())
                && (name == "/" || !name.contains('/'))
        }
    }
}
