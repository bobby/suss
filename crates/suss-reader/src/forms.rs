//! Portable source forms, separate from prototype EDN runtime values.
//!
//! Byte spans refer to the original UTF-8 source. String contents are UTF-16,
//! including lone surrogates; numbers are ordinary binary64, never JVM ratios.
//! Reader metadata remains syntax until phase analysis interprets it. Nesting is
//! bounded to 64 forms to keep diagnostics safe on ordinary thread stacks.
use crate::{Keyword, ParseError, Symbol};
use std::{collections::VecDeque, ops::Range};

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
    /// Ordered feature/body syntax, paired after reader-prefix selection.
    Conditional(Vec<Form>),
    /// Discard awaiting conditional selection of its logical target.
    Discard(Box<Form>),
    /// Reader prefix awaiting a retained target after conditional selection.
    Prefix {
        operator: Box<Form>,
        target: Box<Form>,
    },
}

/// Read portable syntax without namespace resolution or macro execution.
/// Unsupported dispatch/numeric syntax fails explicitly, including ratios and
/// arbitrary-precision suffixes. Empty/comment-only sources yield no forms.
pub fn read_forms(source: &str) -> Result<Vec<Form>, ParseError> {
    read_forms_internal(source, None).map(|(forms, _)| forms)
}

/// Indexing-reader data for compiled macro input. Ordinary compiler forms keep
/// explicit reader metadata only; this opt-in path adds source metadata as data.
pub fn read_forms_with_source_metadata(
    source: &str,
    file: Option<&str>,
) -> Result<Vec<Form>, ParseError> {
    let (forms, carets) = read_forms_internal(source, Some(file))?;
    let mut forms = resolve_conditionals(forms)?;
    source_metadata::list_prefixes(&mut forms, &carets);
    Ok(forms)
}

mod source_metadata;

fn read_forms_internal(
    source: &str,
    locations: Option<Option<&str>>,
) -> Result<(Vec<Form>, source_metadata::Carets), ParseError> {
    let mut reader = Reader {
        source,
        offset: 0,
        depth: 0,
        locations,
        carets: source_metadata::Carets::new(),
        source_positions: locations.map(|_| source_metadata::Positions::new(source)),
    };
    let mut forms = Vec::new();
    loop {
        reader.padding();
        if reader.peek().is_none() {
            return Ok((forms, reader.carets));
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
    resolve_sequence(forms, 0)
}

fn resolve_sequence(forms: Vec<Form>, depth: usize) -> Result<Vec<Form>, ParseError> {
    let mut following = VecDeque::from(forms);
    let mut resolved = Vec::new();
    while let Some(form) = following.pop_front() {
        if let Some(form) = select(form, &mut following, depth)? {
            resolved.push(form);
        }
    }
    Ok(resolved)
}

fn required_selected(
    following: &mut VecDeque<Form>,
    depth: usize,
    span: Range<usize>,
) -> Result<Form, ParseError> {
    while let Some(form) = following.pop_front() {
        if let Some(form) = select(form, following, depth)? {
            return Ok(form);
        }
    }
    Err(error(span, "Reader prefix requires a retained form"))
}

fn pending_prefix(kind: &Kind) -> bool {
    match kind {
        Kind::Conditional(_) | Kind::Discard(_) => true,
        Kind::Prefix { target, .. } => pending_prefix(&target.kind),
        _ => false,
    }
}

// An unselected branch reads its body without selecting nested conditionals.
// Prefix/discard syntax still consumes a logical target within this queue.
fn skip_required(
    following: &mut VecDeque<Form>,
    depth: usize,
    span: Range<usize>,
) -> Result<(), ParseError> {
    while let Some(form) = following.pop_front() {
        if skip_target(form, following, depth)? {
            return Ok(());
        }
    }
    Err(error(span, "Reader prefix requires a retained form"))
}

fn skip_target(
    form: Form,
    following: &mut VecDeque<Form>,
    depth: usize,
) -> Result<bool, ParseError> {
    if depth >= 64 {
        return Err(error(form.span, "Reader selection nesting limit exceeded"));
    }
    let present = match form.kind {
        Kind::Prefix { target, .. } => {
            if !skip_target(*target, following, depth + 1)? {
                skip_required(following, depth + 1, form.span.clone())?;
            }
            true
        }
        Kind::Discard(target) => {
            if !skip_target(*target, following, depth + 1)? {
                skip_required(following, depth + 1, form.span.clone())?;
            }
            false
        }
        _ => true,
    };
    if !present && !form.metadata.is_empty() {
        skip_required(following, depth + 1, form.span)?;
        return Ok(true);
    }
    Ok(present)
}

fn metadata_target(kind: &Kind) -> bool {
    matches!(
        kind,
        Kind::Symbol(_) | Kind::List(_) | Kind::Vector(_) | Kind::Map(_) | Kind::Set(_)
    )
}

fn select(
    mut form: Form,
    following: &mut VecDeque<Form>,
    depth: usize,
) -> Result<Option<Form>, ParseError> {
    if depth >= 64 {
        return Err(error(form.span, "Reader selection nesting limit exceeded"));
    }
    let span = form.span.clone();
    let metadata = std::mem::take(&mut form.metadata);
    let selected = match form.kind {
        Kind::Conditional(clauses) => {
            let mut clauses = VecDeque::from(clauses);
            let mut selected = None;
            while let Some(feature) = clauses.pop_front() {
                let Some(feature) = select(feature, &mut clauses, depth + 1)? else {
                    continue;
                };
                let Kind::Keyword(key) = feature.kind else {
                    return Err(error(feature.span, "Reader feature must be a keyword"));
                };
                if key.namespace.is_none()
                    && matches!(key.name.as_str(), "suss" | "cljs" | "default")
                {
                    selected = Some(required_selected(&mut clauses, depth + 1, span.clone())?);
                    break;
                }
                skip_required(&mut clauses, depth + 1, span.clone())?;
            }
            selected
        }
        Kind::Discard(target) => {
            if select(*target, following, depth + 1)?.is_none() {
                required_selected(following, depth + 1, span.clone())?;
            }
            None
        }
        Kind::Prefix { operator, target } => {
            let target = match select(*target, following, depth + 1)? {
                Some(target) => target,
                None => required_selected(following, depth + 1, span.clone())?,
            };
            form.span.end = target.span.end;
            form.kind = Kind::List(vec![*operator, target]);
            Some(form)
        }
        kind => {
            form.kind = match kind {
                Kind::List(items) => Kind::List(resolve_sequence(items, depth + 1)?),
                Kind::Vector(items) => Kind::Vector(resolve_sequence(items, depth + 1)?),
                Kind::Set(items) => Kind::Set(resolve_sequence(items, depth + 1)?),
                Kind::Map(items) => {
                    let items = resolve_sequence(items, depth + 1)?;
                    if items.len() % 2 != 0 {
                        return Err(error(span, "Conditional leaves an incomplete map entry"));
                    }
                    Kind::Map(items)
                }
                other => other,
            };
            Some(form)
        }
    };
    if metadata.is_empty() {
        return Ok(selected);
    }
    let mut selected = match selected {
        Some(selected) => selected,
        None => required_selected(following, depth + 1, span.clone())?,
    };
    if !metadata_target(&selected.kind) {
        return Err(error(
            selected.span,
            "Metadata requires a symbol or collection",
        ));
    }
    selected
        .metadata
        .splice(0..0, resolve_sequence(metadata, depth + 1)?);
    selected.span.start = span.start;
    Ok(Some(selected))
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
            ',' | '(' | ')' | '[' | ']' | '{' | '}' | '"' | ';' | '`' | '~' | '@' | '^' | '\\'
        )
}

struct Reader<'a> {
    source: &'a str,
    offset: usize,
    depth: usize,
    locations: Option<Option<&'a str>>,
    carets: source_metadata::Carets,
    source_positions: Option<source_metadata::Positions>,
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
    // The REPL uses this recovery hint rather than maintaining another lexer or
    // matching diagnostic prose. Ordinary malformed input has no such hint.
    fn incomplete(&self, start: usize, message: impl Into<String>) -> ParseError {
        let mut error = self.fail(start, message);
        error.expected.push("more input".into());
        error
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
                return Err(self.incomplete(self.offset, "Expected a form before end of input"));
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
                    && !items.iter().any(|item| {
                        matches!(
                            item.kind,
                            Kind::Conditional(_) | Kind::Prefix { .. } | Kind::Discard(_)
                        )
                    })
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
                if self.locations.is_some() {
                    self.carets.insert(
                        (metadata.span.start, metadata.span.end),
                        self.source_positions
                            .as_ref()
                            .unwrap()
                            .position(self.source, start),
                    );
                }
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
                    Kind::Symbol(_)
                        | Kind::List(_)
                        | Kind::Vector(_)
                        | Kind::Map(_)
                        | Kind::Set(_)
                        | Kind::Conditional(_)
                        | Kind::Discard(_)
                        | Kind::Prefix { .. }
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
                    let target = self.required()?;
                    if pending_prefix(&target.kind) {
                        Kind::Discard(Box::new(target))
                    } else {
                        return Ok(None);
                    }
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
                    // Prefixes can consume following raw forms after an unmatched
                    // conditional disappears. Static clauses can be checked now;
                    // dynamic clauses are paired in their isolated selection queue.
                    if !items.iter().any(|item| {
                        matches!(
                            item.kind,
                            Kind::Conditional(_) | Kind::Prefix { .. } | Kind::Discard(_)
                        )
                    }) {
                        if items.len() % 2 != 0 {
                            return Err(
                                self.fail(start, "Reader conditional requires feature/form pairs")
                            );
                        }
                        for pair in items.chunks_exact(2) {
                            if !matches!(pair[0].kind, Kind::Keyword(_)) {
                                return Err(error(
                                    pair[0].span.clone(),
                                    "Reader feature must be a keyword",
                                ));
                            }
                        }
                    }
                    Kind::Conditional(items)
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
        let mut form = Form {
            span: start..self.offset,
            metadata: Vec::new(),
            kind,
        };
        if let Some(file) = self.locations {
            source_metadata::attach(
                self.source,
                self.source_positions.as_ref().unwrap(),
                file,
                &mut form,
            );
        }
        Ok(Some(form))
    }
    fn wrapper(&mut self, start: usize, name: &str) -> Result<Kind, ParseError> {
        let prefix = Form {
            span: start..self.offset,
            metadata: Vec::new(),
            kind: Kind::Symbol(Symbol::new(name)),
        };
        Ok(Kind::Prefix {
            operator: Box::new(prefix),
            target: Box::new(self.required()?),
        })
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
                    return Err(
                        self.incomplete(start, format!("Unclosed collection; expected {end}"))
                    );
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
                None => return Err(self.incomplete(start, "Unclosed string")),
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
                                    // Octal escapes stop at all reader macros,
                                    // including non-token-terminating quote/dispatch.
                                    Some(c) if !delimiter(c) && !matches!(c, '#' | '\'') => {
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
