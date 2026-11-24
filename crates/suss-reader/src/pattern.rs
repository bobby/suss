//! Checked UTF-16 Java regular-expression source for the compiler reader.
//!
//! This is syntax admission and immutable source data, not a matching engine.
//! The parser is original Rust. Property/name facts come from the pinned JVM
//! development oracle; no Java implementation or Java dependency is shipped.
use std::collections::BTreeSet;
mod unicode;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatternSource {
    units: Vec<u16>,
    flags: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub description: String,
    /// UTF-16 offset in the pattern, independently of its enclosing source span.
    pub index: usize,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} near index {}", self.description, self.index)
    }
}
impl std::error::Error for Error {}

impl PatternSource {
    pub fn units(&self) -> &[u16] {
        &self.units
    }
    pub fn flags(&self) -> u32 {
        self.flags
    }
    pub fn compile(units: &[u16]) -> Result<Self, Error> {
        let mut parser = Parser {
            units,
            at: 0,
            flags: 0,
            depth: 0,
            empty_quote: false,
            names: BTreeSet::new(),
        };
        parser.expression(false)?;
        Ok(Self {
            units: units.to_vec(),
            flags: parser.flags,
        })
    }
}

struct Parser<'a> {
    units: &'a [u16],
    at: usize,
    flags: u32,
    depth: usize,
    empty_quote: bool,
    names: BTreeSet<String>,
}
impl Parser<'_> {
    fn error(&self, description: impl Into<String>, index: usize) -> Error {
        Error {
            description: description.into(),
            index,
        }
    }
    fn peek_raw(&self) -> Option<u16> {
        self.units.get(self.at).copied()
    }
    fn take_raw(&mut self) -> Option<u16> {
        let unit = self.peek_raw()?;
        self.at += 1;
        Some(unit)
    }
    fn skip(&mut self) {
        if self.flags & 4 == 0 {
            return;
        }
        loop {
            match self.peek_raw() {
                Some(9..=13 | 32) => {
                    self.at += 1;
                }
                Some(35) => {
                    self.at += 1;
                    while let Some(unit) = self.peek_raw() {
                        self.at += 1;
                        if matches!(unit, 10 | 13 | 0x85 | 0x2028 | 0x2029) {
                            break;
                        }
                    }
                }
                _ => return,
            }
        }
    }
    fn peek(&mut self) -> Option<u16> {
        self.skip();
        self.peek_raw()
    }
    fn take(&mut self) -> Option<u16> {
        self.skip();
        self.take_raw()
    }
    fn expression(&mut self, grouped: bool) -> Result<(), Error> {
        let mut atom = false;
        loop {
            let unit = self.peek();
            let start = self.at;
            match unit {
                None if grouped => return Err(self.error("Unclosed group", self.at)),
                None => return Ok(()),
                Some(41) if grouped => {
                    self.at += 1;
                    return Ok(());
                }
                Some(41) => return Err(self.error("Unmatched closing ')'", self.at)),
                Some(124) => {
                    self.at += 1;
                    atom = false;
                }
                Some(40) => {
                    self.at += 1;
                    atom = self.group()?;
                }
                Some(91) => {
                    self.at += 1;
                    self.class()?;
                    atom = true;
                }
                Some(92) => {
                    self.at += 1;
                    self.escape(false)?;
                    if !self.empty_quote {
                        atom = true;
                    }
                }
                Some(42 | 43 | 63) => {
                    if !atom {
                        return Err(self.error(
                            format!(
                                "Dangling meta character '{}'",
                                char::from_u32(unit.unwrap().into()).unwrap()
                            ),
                            start,
                        ));
                    }
                    self.at += 1;
                    if matches!(self.peek(), Some(63 | 43)) {
                        self.at += 1;
                    }
                    atom = false;
                }
                Some(123) => {
                    self.at += 1;
                    if !atom {
                        return Err(self.error("Illegal repetition", self.at));
                    }
                    self.repetition()?;
                    if matches!(self.peek(), Some(63 | 43)) {
                        self.at += 1;
                    }
                    atom = false;
                }
                Some(_) => {
                    self.at += 1;
                    atom = true;
                }
            }
        }
    }
    fn group(&mut self) -> Result<bool, Error> {
        self.depth += 1;
        if self.depth > 1024 {
            return Err(self.error("Pattern nesting budget exhausted", self.at));
        }
        let flags = self.flags;
        if self.peek_raw() == Some(63) {
            self.at += 1;
            match self.take_raw() {
                Some(58 | 61 | 33 | 62) => {}
                Some(60) => {
                    if matches!(self.peek_raw(), Some(61 | 33)) {
                        self.at += 1;
                    } else {
                        let name = self.group_name()?;
                        if !self.names.insert(name.clone()) {
                            return Err(self.error(
                                format!("Named capturing group <{name}> is already defined"),
                                self.at - 1,
                            ));
                        }
                    }
                }
                Some(_) => {
                    self.at -= 1;
                    let mut removing = false;
                    loop {
                        let unit = self
                            .take_raw()
                            .ok_or_else(|| self.error("Unknown inline modifier", self.at))?;
                        if unit == 45 {
                            if removing {
                                return Err(self.error("Unknown inline modifier", self.at - 1));
                            }
                            removing = true;
                            continue;
                        }
                        let bit = match unit {
                            100 => 1,
                            105 => 2,
                            120 => 4,
                            109 => 8,
                            115 => 32,
                            117 => 64,
                            99 => 128,
                            85 => 320,
                            41 => {
                                self.depth -= 1;
                                return Ok(false);
                            }
                            58 => break,
                            _ => return Err(self.error("Unknown inline modifier", self.at - 1)),
                        };
                        if removing {
                            self.flags &= !bit;
                        } else {
                            self.flags |= bit;
                        }
                    }
                }
                None => return Err(self.error("Unknown inline modifier", self.at)),
            }
        }
        let result = stacker::maybe_grow(64 * 1024, 1024 * 1024, || self.expression(true));
        self.flags = flags;
        self.depth -= 1;
        result.map(|()| true)
    }
    fn group_name(&mut self) -> Result<String, Error> {
        let start = self.at;
        if !matches!(self.peek_raw(), Some(65..=90 | 97..=122)) {
            return Err(self.error(
                "capturing group name does not start with a Latin letter",
                self.at,
            ));
        }
        while matches!(self.peek_raw(), Some(65..=90 | 97..=122 | 48..=57)) {
            self.at += 1;
        }
        if self.take_raw() != Some(62) {
            return Err(self.error(
                "named capturing group is missing trailing '>'",
                self.at.saturating_sub(1),
            ));
        }
        Ok(String::from_utf16(&self.units[start..self.at - 1]).expect("ASCII group name"))
    }
    fn repetition(&mut self) -> Result<(), Error> {
        fn integer(parser: &mut Parser<'_>) -> Result<u32, Error> {
            let mut value = 0u32;
            let start = parser.at;
            while let Some(unit @ 48..=57) = parser.peek() {
                parser.at += 1;
                value = value
                    .checked_mul(10)
                    .and_then(|v| v.checked_add(u32::from(unit - 48)))
                    .filter(|&v| v <= i32::MAX as u32)
                    .ok_or_else(|| parser.error("Illegal repetition range", parser.at - 1))?;
            }
            if parser.at == start {
                return Err(parser.error("Illegal repetition", parser.at));
            }
            Ok(value)
        }
        let min = integer(self)?;
        let mut max = min;
        if self.peek() == Some(44) {
            self.at += 1;
            max = if self.peek() == Some(125) {
                i32::MAX as u32
            } else {
                integer(self)?
            };
        }
        if self.take() != Some(125) {
            return Err(self.error("Unclosed counted closure", self.at.saturating_sub(1)));
        }
        if min > max {
            return Err(self.error("Illegal repetition range", self.at - 1));
        }
        Ok(())
    }
    fn class(&mut self) -> Result<(), Error> {
        self.depth += 1;
        if self.depth > 1024 {
            return Err(self.error("Pattern nesting budget exhausted", self.at));
        }
        if self.peek() == Some(94) {
            self.at += 1;
        }
        let mut first = true;
        loop {
            let Some(unit) = self.peek() else {
                return Err(self.error("Unclosed character class", self.at.saturating_sub(1)));
            };
            if unit == 93 && !first {
                self.at += 1;
                self.depth -= 1;
                self.empty_quote = false;
                return Ok(());
            }
            if unit == 38 && self.units.get(self.at + 1) == Some(&38) {
                self.at += 2;
                continue;
            }
            let start = self.class_atom()?;
            if self.empty_quote {
                continue;
            }
            first = false;
            if start.is_some()
                && self.peek() == Some(45)
                && self.units.get(self.at + 1) != Some(&93)
            {
                self.at += 1;
                let end = self.class_atom()?;
                match (start, end) {
                    (Some(a), Some(b)) if a <= b => {}
                    (Some(_), Some(_)) => {
                        return Err(self.error("Illegal character range", self.at - 1));
                    }
                    _ => {
                        return Err(self.error("Illegal character range", self.at.saturating_sub(1)));
                    }
                }
            }
        }
    }
    fn class_atom(&mut self) -> Result<Option<u32>, Error> {
        self.empty_quote = false;
        let unit = self
            .take()
            .ok_or_else(|| self.error("Unclosed character class", self.at.saturating_sub(1)))?;
        match unit {
            91 => {
                stacker::maybe_grow(64 * 1024, 1024 * 1024, || self.class())?;
                Ok(None)
            }
            92 => self.escape(true),
            0xd800..=0xdbff if matches!(self.peek_raw(), Some(0xdc00..=0xdfff)) => {
                let low = self.take_raw().unwrap();
                Ok(Some(
                    0x10000 + ((u32::from(unit) - 0xd800) << 10) + u32::from(low) - 0xdc00,
                ))
            }
            _ => Ok(Some(u32::from(unit))),
        }
    }
    fn hex(&mut self, count: usize, message: &str) -> Result<u32, Error> {
        let mut value = 0;
        for _ in 0..count {
            let digit = self
                .take_raw()
                .and_then(|u| char::from_u32(u.into()))
                .and_then(|c| c.to_digit(16))
                .ok_or_else(|| self.error(message, self.at.saturating_sub(1)))?;
            value = value * 16 + digit;
        }
        Ok(value)
    }
    fn braced(&mut self, message: &str) -> Result<String, Error> {
        if self.take_raw() != Some(123) {
            return Err(self.error(message, self.at.saturating_sub(1)));
        }
        let start = self.at;
        while !matches!(self.peek_raw(), Some(125) | None) {
            self.at += 1;
        }
        if self.take_raw() != Some(125) {
            return Err(self.error(message, self.at));
        }
        String::from_utf16(&self.units[start..self.at - 1]).map_err(|_| self.error(message, start))
    }
    fn escape(&mut self, in_class: bool) -> Result<Option<u32>, Error> {
        self.empty_quote = false;
        let unit = self
            .take_raw()
            .ok_or_else(|| self.error("Unexpected internal error", self.at))?;
        let at = self.at - 1;
        match unit {
            81 => {
                let mut last = None;
                while let Some(unit) = self.take_raw() {
                    if unit == 92 && self.peek_raw() == Some(69) {
                        self.at += 1;
                        break;
                    }
                    last = Some(
                        if (0xd800..=0xdbff).contains(&unit)
                            && matches!(self.peek_raw(), Some(0xdc00..=0xdfff))
                        {
                            let low = self.take_raw().unwrap();
                            0x10000 + ((u32::from(unit) - 0xd800) << 10) + u32::from(low) - 0xdc00
                        } else {
                            u32::from(unit)
                        },
                    );
                }
                self.empty_quote = last.is_none();
                Ok(last)
            }
            112 | 80 => {
                let property = if self.peek_raw() == Some(123) {
                    self.braced("Unclosed character family")?
                } else {
                    String::from_utf16(&[self
                        .take_raw()
                        .ok_or_else(|| self.error("Unknown character property name", self.at))?])
                    .unwrap_or_default()
                };
                if !unicode::property(&property) {
                    return Err(self.error(
                        format!("Unknown character property name {{{property}}}"),
                        self.at - 1,
                    ));
                }
                Ok(None)
            }
            78 => {
                let name = self.braced("Unclosed character name escape sequence")?;
                let point = unicode::character_name(&name).ok_or_else(|| {
                    self.error(format!("Unknown character name [{name}]"), self.at - 1)
                })?;
                Ok(Some(point))
            }
            107 if !in_class => {
                if self.take_raw() != Some(60) {
                    return Err(self.error(
                        "\\k is not followed by '<' for named capturing group",
                        self.at.saturating_sub(1),
                    ));
                }
                let name = self.group_name()?;
                if !self.names.contains(&name) {
                    return Err(self.error(
                        format!("named capturing group <{name}> does not exist"),
                        self.at - 1,
                    ));
                }
                Ok(None)
            }
            49..=57 if !in_class => {
                while matches!(self.peek_raw(), Some(48..=57)) {
                    self.at += 1;
                }
                Ok(None)
            }
            48 => {
                let first = self
                    .take_raw()
                    .filter(|&u| (48..=55).contains(&u))
                    .ok_or_else(|| {
                        self.error("Illegal octal escape sequence", self.at.saturating_sub(1))
                    })?;
                let mut value = u32::from(first - 48);
                for _ in 0..if first <= 51 { 2 } else { 1 } {
                    if let Some(unit @ 48..=55) = self.peek_raw() {
                        self.at += 1;
                        value = value * 8 + u32::from(unit - 48);
                    } else {
                        break;
                    }
                }
                Ok(Some(value))
            }
            117 => {
                let first = self.hex(4, "Illegal Unicode escape sequence")?;
                if (0xd800..=0xdbff).contains(&first)
                    && self.units.get(self.at..self.at + 2) == Some(&[92, 117])
                {
                    let before = self.at;
                    self.at += 2;
                    if let Ok(low @ 0xdc00..=0xdfff) =
                        self.hex(4, "Illegal Unicode escape sequence")
                    {
                        return Ok(Some(0x10000 + ((first - 0xd800) << 10) + low - 0xdc00));
                    }
                    self.at = before;
                }
                Ok(Some(first))
            }
            120 => {
                if self.peek_raw() == Some(123) {
                    self.at += 1;
                    let start = self.at;
                    let mut value = 0u32;
                    while let Some(unit) = self.peek_raw() {
                        let Some(digit) = char::from_u32(unit.into()).and_then(|c| c.to_digit(16))
                        else {
                            break;
                        };
                        self.at += 1;
                        value = value
                            .checked_mul(16)
                            .and_then(|v| v.checked_add(digit))
                            .filter(|&v| v <= 0x10ffff)
                            .ok_or_else(|| {
                                self.error("Hexadecimal codepoint is too big", self.at - 1)
                            })?;
                    }
                    if self.at == start || self.take_raw() != Some(125) {
                        return Err(self.error(
                            "Unclosed hexadecimal escape sequence",
                            self.at.saturating_sub(1),
                        ));
                    }
                    Ok(Some(value))
                } else {
                    self.hex(2, "Illegal hexadecimal escape sequence").map(Some)
                }
            }
            99 => self
                .take_raw()
                .map(|u| Some(u32::from(u ^ 64)))
                .ok_or_else(|| self.error("Illegal control escape sequence", self.at)),
            116 => Ok(Some(9)),
            110 => Ok(Some(10)),
            114 => Ok(Some(13)),
            102 => Ok(Some(12)),
            97 => Ok(Some(7)),
            101 => Ok(Some(27)),
            100 | 68 | 104 | 72 | 115 | 83 | 118 | 86 | 119 | 87 => Ok(None),
            65 | 71 | 90 | 122 | 66 | 82 | 88 if !in_class => Ok(None),
            98 if !in_class => {
                if self.units.get(self.at..self.at + 3) == Some(&[123, 103, 125]) {
                    self.at += 3;
                }
                Ok(None)
            }
            65..=90 | 97..=122 | 49..=57 => {
                Err(self.error("Illegal/unsupported escape sequence", at))
            }
            _ => Ok(Some(u32::from(unit))),
        }
    }
}
