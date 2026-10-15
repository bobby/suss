//! Portable source forms, separate from prototype EDN runtime values.
//!
//! Byte spans refer to the original UTF-8 source. String contents are UTF-16,
//! including lone surrogates; numbers are ordinary binary64, never JVM ratios.
//! Reader metadata remains syntax until phase analysis interprets it. Reader work
//! is cumulatively metered; deep syntax uses growable stacks and safe cleanup.
use crate::{Keyword, ParseError, Symbol};
use std::{
    collections::{BTreeMap, VecDeque},
    ops::Range,
    sync::atomic::{AtomicU64, Ordering},
};
static ANONYMOUS_IDS: AtomicU64 = AtomicU64::new(1);

pub struct Form {
    pub span: Range<usize>,
    /// Prefix metadata in source order, with each metadata form's own span.
    pub metadata: Vec<Form>,
    pub kind: Kind,
}

impl std::fmt::Debug for Form {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
            f.debug_struct("Form").field("span", &self.span)
                .field("metadata", &self.metadata).field("kind", &self.kind).finish()
        })
    }
}
impl Form {
    /// Move the kind out while leaving a valid, shallow value for Drop.
    pub fn take_kind(&mut self) -> Kind {
        std::mem::replace(&mut self.kind, Kind::Nil)
    }
    pub fn into_parts(mut self) -> (Range<usize>, Vec<Form>, Kind) {
        (
            std::mem::take(&mut self.span),
            std::mem::take(&mut self.metadata),
            self.take_kind(),
        )
    }
    pub fn clone_metered(&self, meter: &mut WorkMeter<'_>) -> Result<Self, ParseError> {
        let text = match &self.kind {
            Kind::String(x) => x.len() as u64,
            Kind::Symbol(x) => (x.name.len() + x.namespace.as_ref().map_or(0, String::len)) as u64,
            Kind::Keyword(x) => (x.name.len() + x.namespace.as_ref().map_or(0, String::len)) as u64,
            _ => 0,
        };
        charge(meter, 1 + text, self.span.clone())?;
        stacker::maybe_grow(256 * 1024, 4 * 1024 * 1024, || {
            let metadata = clone_items(&self.metadata, meter)?;
            let kind = match &self.kind {
                Kind::List(x) => Kind::List(clone_items(x, meter)?),
                Kind::Vector(x) => Kind::Vector(clone_items(x, meter)?),
                Kind::Set(x) => Kind::Set(clone_items(x, meter)?),
                Kind::Map(x) => Kind::Map(clone_items(x, meter)?),
                Kind::Conditional(x) => Kind::Conditional(clone_items(x, meter)?),
                Kind::SplicingConditional(x) => Kind::SplicingConditional(clone_items(x, meter)?),
                Kind::Discard(x) => Kind::Discard(Box::new(x.clone_metered(meter)?)),
                Kind::Prefix { operator, target } => Kind::Prefix {
                    operator: Box::new(operator.clone_metered(meter)?),
                    target: Box::new(target.clone_metered(meter)?),
                },
                other => other.clone(),
            };
            Ok(Self {
                span: self.span.clone(),
                metadata,
                kind,
            })
        })
    }
}
impl PartialEq for Form {
    fn eq(&self, other: &Self) -> bool {
        stacker::maybe_grow(256 * 1024, 4 * 1024 * 1024, || {
            self.span == other.span && self.metadata == other.metadata && self.kind == other.kind
        })
    }
}
impl Clone for Form {
    fn clone(&self) -> Self {
        self.clone_metered(&mut |_| Ok(()))
            .expect("infallible Form clone")
    }
}
fn clone_items(items: &[Form], meter: &mut WorkMeter<'_>) -> Result<Vec<Form>, ParseError> {
    items.iter().map(|item| item.clone_metered(meter)).collect()
}
impl Drop for Form {
    fn drop(&mut self) {
        fn children(kind: Kind, pending: &mut Vec<Form>) {
            match kind {
                Kind::List(x)
                | Kind::Vector(x)
                | Kind::Set(x)
                | Kind::Map(x)
                | Kind::Conditional(x)
                | Kind::SplicingConditional(x) => pending.extend(x),
                Kind::Discard(x) => pending.push(*x),
                Kind::Prefix { operator, target } => {
                    pending.push(*operator);
                    pending.push(*target);
                }
                _ => {}
            }
        }
        if self.metadata.is_empty() && matches!(self.kind, Kind::Nil) {
            return;
        }
        let mut pending = std::mem::take(&mut self.metadata);
        children(self.take_kind(), &mut pending);
        while let Some(mut form) = pending.pop() {
            pending.append(&mut form.metadata);
            children(form.take_kind(), &mut pending);
            // This now-empty Form drops without recursive ownership traversal.
        }
    }
}

pub type WorkMeter<'a> = dyn FnMut(u64) -> Result<(), ParseError> + 'a;
pub const DEFAULT_WORK_ALLOWANCE: u64 = 10_000_000;
fn charge(meter: &mut WorkMeter<'_>, units: u64, span: Range<usize>) -> Result<(), ParseError> {
    meter(units).map_err(|mut failure| {
        // Preserve a caller-supplied cancellation/error, assigning a location
        // only when its callback deliberately has no reader location.
        if failure.span == (0..0) {
            failure.span = span;
        }
        failure
    })
}
fn allowance(remaining: &mut u64, units: u64) -> Result<(), ParseError> {
    *remaining = remaining
        .checked_sub(units)
        .ok_or_else(|| error(0..0, "Reader work budget exhausted"))?;
    Ok(())
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
    SplicingConditional(Vec<Form>),
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
    read_forms_with_work_allowance(source, DEFAULT_WORK_ALLOWANCE)
}
pub fn read_forms_with_work_allowance(
    source: &str,
    mut work: u64,
) -> Result<Vec<Form>, ParseError> {
    read_forms_metered(source, &mut |units| allowance(&mut work, units))
}
pub fn read_forms_metered(
    source: &str,
    meter: &mut WorkMeter<'_>,
) -> Result<Vec<Form>, ParseError> {
    read_forms_internal(source, None, meter).map(|(forms, _)| forms)
}

/// Indexing-reader data with the complete ordinary source metadata retained.
pub fn read_forms_with_source_metadata(
    source: &str,
    file: Option<&str>,
) -> Result<Vec<Form>, ParseError> {
    let mut work = DEFAULT_WORK_ALLOWANCE;
    read_forms_with_source_metadata_metered(source, file, &mut |units| allowance(&mut work, units))
}
pub fn read_forms_with_source_metadata_metered(
    source: &str,
    file: Option<&str>,
    meter: &mut WorkMeter<'_>,
) -> Result<Vec<Form>, ParseError> {
    let (forms, carets) = read_forms_internal(source, Some(file), meter)?;
    let mut forms = resolve_conditionals_metered(forms, meter)?;
    source_metadata::list_prefixes_metered(&mut forms, &carets, meter)?;
    Ok(forms)
}

mod source_metadata;

fn read_forms_internal(
    source: &str,
    locations: Option<Option<&str>>,
    meter: &mut WorkMeter<'_>,
) -> Result<(Vec<Form>, source_metadata::Carets), ParseError> {
    let source_positions = if locations.is_some() {
        charge(meter, source.len() as u64, 0..source.len())?;
        Some(source_metadata::Positions::new(source))
    } else {
        None
    };
    let mut reader = Reader {
        source,
        offset: 0,
        delimited_depth: 0,
        anonymous: None,
        logical_pending: VecDeque::new(),
        locations,
        carets: source_metadata::Carets::new(),
        source_positions,
        meter,
        work_failure: None,
    };
    let result = (|| {
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
    })();
    if let Some(failure) = reader.work_failure {
        return Err(failure);
    }
    result.map(|forms| (forms, reader.carets))
}

/// Resolve the first matching portable clause in source order, cumulatively metered.
pub fn resolve_conditionals(forms: Vec<Form>) -> Result<Vec<Form>, ParseError> {
    resolve_conditionals_with_work_allowance(forms, DEFAULT_WORK_ALLOWANCE)
}
pub fn resolve_conditionals_with_work_allowance(
    forms: Vec<Form>,
    mut work: u64,
) -> Result<Vec<Form>, ParseError> {
    resolve_conditionals_metered(forms, &mut |units| allowance(&mut work, units))
}
pub fn resolve_conditionals_metered(
    forms: Vec<Form>,
    meter: &mut WorkMeter<'_>,
) -> Result<Vec<Form>, ParseError> {
    resolve_sequence(forms, 0, meter)
}

fn resolve_sequence(
    forms: Vec<Form>,
    depth: usize,
    meter: &mut WorkMeter<'_>,
) -> Result<Vec<Form>, ParseError> {
    let mut following = VecDeque::from(forms);
    let mut resolved = Vec::new();
    while let Some(form) = following.pop_front() {
        if let Some(form) =
            select(form, &mut following, depth, meter).map_err(closed_selection_error)?
        {
            resolved.push(form);
        }
    }
    Ok(resolved)
}

fn closed_selection_error(mut failure: ParseError) -> ParseError {
    failure
        .expected
        .retain(|hint| hint != "logical continuation");
    failure
}

fn required_selected(
    following: &mut VecDeque<Form>,
    depth: usize,
    span: Range<usize>,
    meter: &mut WorkMeter<'_>,
) -> Result<Form, ParseError> {
    while let Some(form) = following.pop_front() {
        if let Some(form) = select(form, following, depth, meter)? {
            return Ok(form);
        }
    }
    Err({
        let mut failure = error(span, "Reader prefix requires a retained form");
        failure.expected.push("logical continuation".into());
        failure
    })
}

fn pending_prefix(
    mut kind: &Kind,
    meter: &mut WorkMeter<'_>,
    span: Range<usize>,
) -> Result<bool, ParseError> {
    loop {
        charge(meter, 1, span.clone())?;
        match kind {
            Kind::Conditional(_) | Kind::SplicingConditional(_) | Kind::Discard(_) => {
                return Ok(true);
            }
            Kind::Prefix { target, .. } => kind = &target.kind,
            _ => return Ok(false),
        }
    }
}

// An unselected branch reads its body without selecting nested conditionals.
// Prefix/discard syntax still consumes a logical target within this queue.
fn skip_required(
    following: &mut VecDeque<Form>,
    depth: usize,
    span: Range<usize>,
    meter: &mut WorkMeter<'_>,
) -> Result<(), ParseError> {
    while let Some(form) = following.pop_front() {
        if skip_target(form, following, depth, meter)? {
            return Ok(());
        }
    }
    Err({
        let mut failure = error(span, "Reader prefix requires a retained form");
        failure.expected.push("logical continuation".into());
        failure
    })
}

fn skip_target(
    mut form: Form,
    following: &mut VecDeque<Form>,
    depth: usize,
    meter: &mut WorkMeter<'_>,
) -> Result<bool, ParseError> {
    charge(meter, 1, form.span.clone())?;
    stacker::maybe_grow(256 * 1024, 4 * 1024 * 1024, || {
        skip_target_inner(form, following, depth, meter)
    })
}
fn skip_target_inner(
    mut form: Form,
    following: &mut VecDeque<Form>,
    depth: usize,
    meter: &mut WorkMeter<'_>,
) -> Result<bool, ParseError> {
    let present = match form.take_kind() {
        Kind::Prefix { target, .. } => {
            if !skip_target(*target, following, depth + 1, meter)? {
                skip_required(following, depth + 1, form.span.clone(), meter)?;
            }
            true
        }
        Kind::Discard(target) => {
            if !skip_target(*target, following, depth + 1, meter)? {
                skip_required(following, depth + 1, form.span.clone(), meter)?;
            }
            false
        }
        _ => true,
    };
    if !present && !form.metadata.is_empty() {
        skip_required(following, depth + 1, form.span.clone(), meter)?;
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
    meter: &mut WorkMeter<'_>,
) -> Result<Option<Form>, ParseError> {
    charge(meter, 1, form.span.clone())?;
    stacker::maybe_grow(256 * 1024, 4 * 1024 * 1024, || {
        select_inner(form, following, depth, meter)
    })
}
fn select_inner(
    mut form: Form,
    following: &mut VecDeque<Form>,
    depth: usize,
    meter: &mut WorkMeter<'_>,
) -> Result<Option<Form>, ParseError> {
    let span = form.span.clone();
    let metadata = std::mem::take(&mut form.metadata);
    let selected = match form.take_kind() {
        Kind::SplicingConditional(clauses) => {
            let raw = Form {
                span: span.clone(),
                metadata: vec![],
                kind: Kind::Conditional(clauses),
            };
            if let Some(mut selected) = select(raw, following, depth + 1, meter)? {
                let items = match selected.take_kind() {
                    Kind::List(items) | Kind::Vector(items) => items,
                    _ => {
                        return Err(error(
                            span,
                            "Splicing conditional requires a sequential value",
                        ));
                    }
                };
                for item in items.into_iter().rev() {
                    following.push_front(item);
                }
            }
            None
        }
        Kind::Conditional(clauses) => (|| -> Result<Option<Form>, ParseError> {
            let mut clauses = VecDeque::from(clauses);
            let mut selected = None;
            while let Some(feature) = clauses.pop_front() {
                let Some(mut feature) = select(feature, &mut clauses, depth + 1, meter)? else {
                    continue;
                };
                let Kind::Keyword(key) = feature.take_kind() else {
                    return Err(error(
                        feature.span.clone(),
                        "Reader feature must be a keyword",
                    ));
                };
                if key.namespace.is_none()
                    && matches!(key.name.as_str(), "suss" | "cljs" | "default")
                {
                    selected = Some(required_selected(
                        &mut clauses,
                        depth + 1,
                        span.clone(),
                        meter,
                    )?);
                    break;
                }
                skip_required(&mut clauses, depth + 1, span.clone(), meter)?;
            }
            Ok(selected)
        })()
        .map_err(closed_selection_error)?,
        Kind::Discard(target) => {
            if select(*target, following, depth + 1, meter)?.is_none() {
                required_selected(following, depth + 1, span.clone(), meter)?;
            }
            None
        }
        Kind::Prefix { operator, target } => {
            let target = match select(*target, following, depth + 1, meter)? {
                Some(target) => target,
                None => required_selected(following, depth + 1, span.clone(), meter)?,
            };
            form.span.end = target.span.end;
            form.kind = Kind::List(vec![*operator, target]);
            Some(form)
        }
        kind => {
            form.kind = match kind {
                Kind::List(items) => Kind::List(resolve_sequence(items, depth + 1, meter)?),
                Kind::Vector(items) => Kind::Vector(resolve_sequence(items, depth + 1, meter)?),
                Kind::Set(items) => Kind::Set(resolve_sequence(items, depth + 1, meter)?),
                Kind::Map(items) => {
                    let items = resolve_sequence(items, depth + 1, meter)?;
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
        None => required_selected(following, depth + 1, span.clone(), meter)?,
    };
    if !metadata_target(&selected.kind) {
        return Err(error(
            selected.span.clone(),
            "Metadata requires a symbol or collection",
        ));
    }
    selected
        .metadata
        .splice(0..0, resolve_sequence(metadata, depth + 1, meter)?);
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

struct Reader<'a, 'm, 'f> {
    source: &'a str,
    offset: usize,
    delimited_depth: usize,
    anonymous: Option<(u64, BTreeMap<i32, Symbol>)>,
    logical_pending: VecDeque<Form>,
    locations: Option<Option<&'a str>>,
    carets: source_metadata::Carets,
    source_positions: Option<source_metadata::Positions>,
    meter: &'m mut WorkMeter<'f>,
    work_failure: Option<ParseError>,
}
impl Reader<'_, '_, '_> {
    fn anonymous_symbol(&mut self, index: i32) -> Symbol {
        let (id, arguments) = self.anonymous.as_mut().unwrap();
        arguments
            .entry(index)
            .or_insert_with(|| {
                Symbol::new(if index == -1 {
                    format!("rest__{id}#")
                } else {
                    format!("p{index}__{id}#")
                })
            })
            .clone()
    }
    fn anonymous_index_value(&self, token: &str, start: usize) -> Result<i32, ParseError> {
        let token = token;
        let token = token.strip_suffix('N').unwrap_or(token);
        let (negative, digits) = if let Some(t) = token.strip_prefix('-') {
            (true, t)
        } else {
            (false, token.strip_prefix('+').unwrap_or(token))
        };
        let (radix, digits) = if let Some(t) = digits
            .strip_prefix("0x")
            .or_else(|| digits.strip_prefix("0X"))
        {
            (16, t)
        } else if let Some((radix, t)) = digits.split_once('r').or_else(|| digits.split_once('R')) {
            (radix.parse::<u32>().unwrap_or(0), t)
        } else if digits.len() > 1 && digits.starts_with('0') {
            (8, &digits[1..])
        } else {
            (10, digits)
        };
        if !(2..=36).contains(&radix) {
            return Err(self.fail(start, "Invalid anonymous argument index"));
        }
        let value = i64::from_str_radix(digits, radix)
            .map_err(|_| self.fail(start, "Invalid anonymous argument index"))?;
        let value = if negative { -value } else { value };
        i32::try_from(value).map_err(|_| {
            self.fail(
                start,
                "Anonymous argument index is not representable as i32",
            )
        })
    }
    fn anonymous_argument(&mut self, start: usize) -> Result<Symbol, ParseError> {
        let index = match self.peek() {
            None => 1,
            Some(c) if delimiter(c) || c == '\'' => 1,
            Some('&') => {
                self.take();
                -1
            }
            Some('#') => {
                let mut pending = std::mem::take(&mut self.logical_pending);
                let selected = loop {
                    let raw = match pending.pop_front() {
                        Some(form) => form,
                        None => self.required()?,
                    };
                    let selected = loop {
                        let mut trial = pending
                            .iter()
                            .map(|form| form.clone_metered(self.meter))
                            .collect::<Result<VecDeque<_>, _>>()?;
                        match select(raw.clone_metered(self.meter)?, &mut trial, 0, self.meter) {
                            Ok(selected) => {
                                pending = trial;
                                break selected;
                            }
                            Err(failure)
                                if failure
                                    .expected
                                    .iter()
                                    .any(|hint| hint == "logical continuation") =>
                            {
                                pending.push_back(self.required()?);
                            }
                            Err(failure) => return Err(failure),
                        }
                    };
                    if let Some(form) = selected {
                        break form;
                    }
                };
                self.logical_pending = pending;
                if !matches!(selected.kind, Kind::Number(_)) {
                    return Err(self.fail(start, "Anonymous argument index requires an integer"));
                }
                self.anonymous_index_value(&self.source[selected.span.clone()], start)?
            }
            _ => {
                let token = self.token().to_owned();
                self.anonymous_index_value(&token, start)?
            }
        };
        Ok(self.anonymous_symbol(index))
    }
    fn anonymous_function(&mut self, start: usize) -> Result<Kind, ParseError> {
        if self.anonymous.is_some() {
            return Err(self.fail(start, "Nested anonymous function literals are not allowed"));
        }
        self.anonymous = Some((
            ANONYMOUS_IDS.fetch_add(1, Ordering::Relaxed),
            BTreeMap::new(),
        ));
        let body = self.items(start, ')');
        if body.is_err() {
            self.anonymous = None;
        }
        let body = body?;
        let maximum = self
            .anonymous
            .as_ref()
            .unwrap()
            .1
            .keys()
            .copied()
            .max()
            .unwrap_or(0)
            .max(0);
        // Charge all generated slots before allocating or populating the vector.
        let rest_slots = if self.anonymous.as_ref().unwrap().1.contains_key(&-1) {
            2
        } else {
            0
        };
        charge(
            self.meter,
            maximum as u64 + rest_slots + 3,
            start..self.offset,
        )?;
        let mut arguments = Vec::new();
        for index in 1..=maximum {
            let symbol = self.anonymous_symbol(index);
            arguments.push(Form {
                span: start..start + 2,
                metadata: vec![],
                kind: Kind::Symbol(symbol),
            });
        }
        if self.anonymous.as_ref().unwrap().1.contains_key(&-1) {
            arguments.push(Form {
                span: start..start + 2,
                metadata: vec![],
                kind: Kind::Symbol(Symbol::new("&")),
            });
            arguments.push(Form {
                span: start..start + 2,
                metadata: vec![],
                kind: Kind::Symbol(self.anonymous_symbol(-1)),
            });
        }
        self.anonymous = None;
        Ok(Kind::List(vec![
            Form {
                span: start..start + 2,
                metadata: vec![],
                kind: Kind::Symbol(Symbol::new("fn*")),
            },
            Form {
                span: start..start + 2,
                metadata: vec![],
                kind: Kind::Vector(arguments),
            },
            {
                let mut form = Form {
                    span: start + 1..self.offset,
                    metadata: vec![],
                    kind: Kind::List(body),
                };
                if let Some(file) = self.locations {
                    source_metadata::attach_metered(
                        self.source,
                        self.source_positions.as_ref().unwrap(),
                        file,
                        &mut form,
                        self.meter,
                    )?;
                }
                form
            },
        ]))
    }

    fn peek(&self) -> Option<char> {
        if self.work_failure.is_some() {
            return None;
        }
        self.source[self.offset..].chars().next()
    }
    fn take(&mut self) -> Option<char> {
        let c = self.peek()?;
        if let Err(failure) = charge(
            self.meter,
            c.len_utf8() as u64,
            self.offset..self.offset + c.len_utf8(),
        ) {
            self.work_failure = Some(failure);
            return None;
        }
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
        if let Some(form) = self.logical_pending.pop_front() {
            return Ok(Some(form));
        }
        self.padding();
        charge(self.meter, 1, self.offset..self.offset)?;
        stacker::maybe_grow(256 * 1024, 4 * 1024 * 1024, || self.form_inner())
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
                            Kind::Conditional(_)
                                | Kind::SplicingConditional(_)
                                | Kind::Prefix { .. }
                                | Kind::Discard(_)
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
                        metadata.span.clone(),
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
                        | Kind::SplicingConditional(_)
                        | Kind::Discard(_)
                        | Kind::Prefix { .. }
                ) {
                    return Err(error(
                        target.span.clone(),
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
            '%' if self.anonymous.is_some() => {
                let symbol = self.anonymous_argument(start)?;
                return Ok(Some(Form {
                    span: start..self.offset,
                    metadata: vec![],
                    kind: Kind::Symbol(symbol),
                }));
            }
            '#' => match self.take() {
                Some('(') => {
                    let kind = self.anonymous_function(start)?;
                    return Ok(Some(Form {
                        span: start..self.offset,
                        metadata: vec![],
                        kind,
                    }));
                }
                Some('{') => Kind::Set(self.items(start, '}')?),
                Some('_') => {
                    let target = self.required()?;
                    if pending_prefix(&target.kind, self.meter, target.span.clone())? {
                        Kind::Discard(Box::new(target))
                    } else {
                        return Ok(None);
                    }
                }
                Some('\'') => self.wrapper(start, "var")?,
                Some('?') => {
                    let splicing = self.peek() == Some('@');
                    if splicing {
                        self.take();
                        if self.delimited_depth == 0 {
                            return Err(self.fail(
                                start,
                                "Splicing conditional requires a delimited collection",
                            ));
                        }
                    }
                    self.padding();
                    if self.take() != Some('(') {
                        return Err(self.fail(start, "Reader conditional requires a list"));
                    }
                    let items = self.items_inner(start, ')')?;
                    // Prefixes can consume following raw forms after an unmatched
                    // conditional disappears. Static clauses can be checked now;
                    // dynamic clauses are paired in their isolated selection queue.
                    if !items.iter().any(|item| {
                        matches!(
                            item.kind,
                            Kind::Conditional(_)
                                | Kind::SplicingConditional(_)
                                | Kind::Prefix { .. }
                                | Kind::Discard(_)
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
                    if splicing {
                        Kind::SplicingConditional(items)
                    } else {
                        Kind::Conditional(items)
                    }
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
                Some('j') => {
                    if self.take() != Some('s') {
                        return Err(self.fail(start, "Unsupported reader dispatch"));
                    }
                    self.padding();
                    if self.take() != Some('{') {
                        return Err(self.fail(start, "#js requires a map literal"));
                    }
                    let mut entries = self.items(start, '}')?;
                    if entries.len() % 2 != 0 {
                        return Err(self.fail(start, "#js map requires an even number of forms"));
                    }
                    for key in entries.iter_mut().step_by(2) {
                        let spelling = match &key.kind {
                            Kind::Keyword(keyword) => match &keyword.namespace {
                                Some(namespace) => format!("{namespace}/{}", keyword.name),
                                None => keyword.name.clone(),
                            },
                            Kind::Symbol(symbol) => match &symbol.namespace {
                                Some(namespace) => format!("{namespace}/{}", symbol.name),
                                None => symbol.name.clone(),
                            },
                            Kind::String(units) => String::from_utf16(units).map_err(|_| {
                                self.fail(key.span.start, "#js key is not valid UTF-16")
                            })?,
                            _ => {
                                return Err(error(
                                    key.span.clone(),
                                    "#js keys must be keywords, symbols, or strings",
                                ));
                            }
                        };
                        key.kind = Kind::String(spelling.encode_utf16().collect());
                        key.metadata.clear();
                    }
                    let operator = Form {
                        span: start..(start + 3).min(self.offset),
                        metadata: Vec::new(),
                        kind: Kind::Symbol(Symbol::new("js-obj")),
                    };
                    let mut call = Vec::with_capacity(entries.len() + 1);
                    call.push(operator);
                    call.extend(entries);
                    return Ok(Some(Form {
                        span: start..self.offset,
                        metadata: Vec::new(),
                        kind: Kind::List(call),
                    }));
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
            source_metadata::attach_metered(
                self.source,
                self.source_positions.as_ref().unwrap(),
                file,
                &mut form,
                self.meter,
            )?;
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
        self.delimited_depth += 1;
        let result = self.items_inner(start, end);
        self.delimited_depth -= 1;
        result
    }
    fn items_inner(&mut self, start: usize, end: char) -> Result<Vec<Form>, ParseError> {
        let mut items = Vec::new();
        loop {
            if let Some(form) = self.logical_pending.pop_front() {
                items.push(form);
                continue;
            }
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
