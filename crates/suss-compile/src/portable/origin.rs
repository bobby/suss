//! Immutable source provenance for compiler and compiled macro facts.
use std::{
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};
mod form_metadata;
pub(crate) use form_metadata::clone_form_metered;

#[derive(Debug, Clone)]
pub struct SourceOrigin {
    text: Arc<str>,
    path: Option<PathBuf>,
    line_starts: Arc<[usize]>,
    metadata_forms: Arc<OnceLock<Result<form_metadata::MetadataSnapshot, String>>>,
    // Private, immutable correspondence installed only by verified reader
    // expansion/lowering. It never mutates the original cached source snapshot.
    reader_forms: Option<Arc<(suss_reader::forms::Form, suss_reader::forms::Form)>>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourcePosition {
    pub line: usize,
    /// One-based UTF-16 code-unit column, independent of UTF-8 byte width.
    pub column: usize,
}
fn equal_text_metered(
    left: &str,
    right: &str,
    meter: &mut impl FnMut(u64) -> Result<(), super::Diagnostic>,
) -> Result<bool, super::Diagnostic> {
    if left.len() != right.len() {
        return Ok(false);
    }
    for (left, right) in left
        .as_bytes()
        .chunks(256)
        .zip(right.as_bytes().chunks(256))
    {
        meter(u64::try_from(left.len()).unwrap_or(u64::MAX))?;
        if left != right {
            return Ok(false);
        }
    }
    Ok(true)
}
impl SourceOrigin {
    /// Native immutable source-owner identity, not a source-content verdict.
    /// Callers must separately validate the complete associated input facts.
    pub(crate) fn same_native_owner(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.text, &other.text)
            && Arc::ptr_eq(&self.line_starts, &other.line_starts)
            && Arc::ptr_eq(&self.metadata_forms, &other.metadata_forms)
            && match (&self.reader_forms, &other.reader_forms) {
                (None, None) => true,
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                _ => false,
            }
    }

    /// Builds the UTF-8 line-start index eagerly. This constructor has no
    /// operation meter; macro-expansion callers should charge the source byte
    /// length at their operation boundary before creating the origin.
    pub fn new(text: impl Into<Arc<str>>, path: Option<PathBuf>) -> Self {
        let text = text.into();
        let bytes = text.as_bytes();
        let mut starts = vec![0];
        let mut offset = 0;
        while offset < bytes.len() {
            match bytes[offset] {
                b'\r' => {
                    offset += 1;
                    if bytes.get(offset) == Some(&b'\n') {
                        offset += 1;
                    }
                    starts.push(offset);
                }
                b'\n' => {
                    offset += 1;
                    starts.push(offset);
                }
                _ => offset += 1,
            }
        }
        Self {
            text,
            path,
            line_starts: starts.into(),
            metadata_forms: Arc::new(OnceLock::new()),
            reader_forms: None,
        }
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
    /// Position of a source symbol token, excluding its metadata prefixes.
    /// Expansion declarations without matching source syntax remain unknown.
    pub fn symbol_position(&self, form: &suss_reader::forms::Form) -> Option<SourcePosition> {
        use suss_reader::forms::{Kind, read_forms, resolve_conditionals};
        let Kind::Symbol(symbol) = &form.kind else {
            return None;
        };
        let source = self.text.get(form.span.clone())?;
        let parsed = resolve_conditionals(read_forms(source).ok()?).ok()?;
        if parsed.len() != 1 || parsed[0].kind != form.kind {
            return None;
        }
        let spelling = symbol.to_string();
        let end = form.span.start.checked_add(parsed[0].span.end)?;
        let start = end.checked_sub(spelling.len())?;
        if self.text.get(start..end)? != spelling {
            return None;
        }
        self.position(start)
    }

    /// Reconstruct a source symbol position while charging reader work,
    /// spelling construction, and line/column lookup to the caller's budget.
    /// Invalid or nonmatching source syntax remains `Ok(None)`; meter failures
    /// are returned so callers cannot silently omit provenance on exhaustion.
    pub(crate) fn symbol_position_metered(
        &self,
        form: &suss_reader::forms::Form,
        meter: &mut impl FnMut(u64) -> Result<(), super::Diagnostic>,
    ) -> Result<Option<SourcePosition>, super::Diagnostic> {
        use suss_reader::forms::{Kind, read_forms_metered, resolve_conditionals_metered};
        let Kind::Symbol(symbol) = &form.kind else {
            return Ok(None);
        };
        let Some(source) = self.text.get(form.span.clone()) else {
            return Ok(None);
        };
        let mut meter_failure = None;
        let mut reader_meter = |work| match meter(work) {
            Ok(()) => Ok(()),
            Err(error) => {
                let parse_error = suss_reader::ParseError {
                    span: error.span.clone(),
                    message: error.message.clone(),
                    expected: vec![],
                };
                meter_failure = Some(error);
                Err(parse_error)
            }
        };
        let parsed = match read_forms_metered(source, &mut reader_meter) {
            Ok(parsed) => parsed,
            Err(_error) => {
                drop(reader_meter);
                return match meter_failure {
                    Some(error) => Err(error),
                    None => Ok(None),
                };
            }
        };
        let parsed = match resolve_conditionals_metered(parsed, &mut reader_meter) {
            Ok(parsed) => parsed,
            Err(_error) => {
                drop(reader_meter);
                return match meter_failure {
                    Some(error) => Err(error),
                    None => Ok(None),
                };
            }
        };
        drop(reader_meter);
        if parsed.len() != 1 {
            return Ok(None);
        }
        let Kind::Symbol(parsed_symbol) = &parsed[0].kind else {
            return Ok(None);
        };
        let same_namespace = match (&parsed_symbol.namespace, &symbol.namespace) {
            (Some(left), Some(right)) => equal_text_metered(left, right, meter)?,
            (None, None) => true,
            _ => false,
        };
        if !same_namespace || !equal_text_metered(&parsed_symbol.name, &symbol.name, meter)? {
            return Ok(None);
        }
        let spelling_len =
            symbol
                .namespace
                .as_ref()
                .map_or(Ok(symbol.name.len()), |namespace| {
                    namespace
                        .len()
                        .checked_add(1)
                        .and_then(|length| length.checked_add(symbol.name.len()))
                        .ok_or_else(|| super::Diagnostic {
                            span: form.span.clone(),
                            message: "Source symbol spelling length overflowed".into(),
                        })
                })?;
        meter(u64::try_from(spelling_len).unwrap_or(u64::MAX))?;
        let spelling = symbol.to_string();
        let Some(end) = form.span.start.checked_add(parsed[0].span.end) else {
            return Ok(None);
        };
        let Some(start) = end.checked_sub(spelling.len()) else {
            return Ok(None);
        };
        let Some(source_spelling) = self.text.get(start..end) else {
            return Ok(None);
        };
        if source_spelling.len() != spelling.len() {
            return Ok(None);
        }
        if !equal_text_metered(source_spelling, &spelling, meter)? {
            return Ok(None);
        }
        self.position_metered(start, &form.span, meter)
    }

    fn position_metered(
        &self,
        byte_offset: usize,
        span: &std::ops::Range<usize>,
        meter: &mut impl FnMut(u64) -> Result<(), super::Diagnostic>,
    ) -> Result<Option<SourcePosition>, super::Diagnostic> {
        if !self.text.is_char_boundary(byte_offset) {
            return Ok(None);
        }
        // Upper-bound binary search, charging each comparison before doing it.
        let (mut low, mut high) = (0, self.line_starts.len());
        while low < high {
            meter(1)?;
            let middle = low + (high - low) / 2;
            if self.line_starts[middle] <= byte_offset {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        let line_index = low.checked_sub(1).ok_or_else(|| super::Diagnostic {
            span: span.clone(),
            message: "Source line index is unavailable".into(),
        })?;
        let Some(prefix) = self.text.get(self.line_starts[line_index]..byte_offset) else {
            return Ok(None);
        };
        meter(u64::try_from(prefix.len()).unwrap_or(u64::MAX))?;
        Ok(Some(SourcePosition {
            line: line_index + 1,
            column: prefix.encode_utf16().count() + 1,
        }))
    }
    pub fn position(&self, byte_offset: usize) -> Option<SourcePosition> {
        if !self.text.is_char_boundary(byte_offset) {
            return None;
        }
        let line_index = self
            .line_starts
            .partition_point(|start| *start <= byte_offset)
            - 1;
        let prefix = self.text.get(self.line_starts[line_index]..byte_offset)?;
        Some(SourcePosition {
            line: line_index + 1,
            column: prefix.encode_utf16().count() + 1,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use suss_reader::forms::{Kind, read_forms};

    #[test]
    fn metered_symbol_position_excludes_metadata_and_counts_utf16_columns() {
        let source = "\"😀\" ^{:tag true} ns/name";
        let form = read_forms(source).unwrap().remove(1);
        let origin = SourceOrigin::new(source, None);
        let mut charged = 0u64;
        let position = origin
            .symbol_position_metered(&form, &mut |work| {
                charged += work;
                Ok(())
            })
            .unwrap()
            .unwrap();
        assert_eq!(
            position,
            SourcePosition {
                line: 1,
                column: 19
            }
        );
        assert!(charged > 0);
        assert_eq!(origin.symbol_position(&form), Some(position));
    }

    #[test]
    fn metered_symbol_position_keeps_invalid_source_unknown() {
        let form = read_forms("name").unwrap().remove(0);
        let mismatching_origin = SourceOrigin::new("nope", None);
        assert_eq!(
            mismatching_origin
                .symbol_position_metered(&form, &mut |_| Ok(()))
                .unwrap(),
            None
        );
        let invalid_syntax = SourceOrigin::new("(", None);
        let invalid_symbol = suss_reader::forms::Form {
            span: 0..1,
            metadata: vec![],
            kind: Kind::Symbol(suss_reader::Symbol::new("name")),
        };
        assert_eq!(
            invalid_syntax
                .symbol_position_metered(&invalid_symbol, &mut |_| Ok(()))
                .unwrap(),
            None
        );

        let origin = SourceOrigin::new("name", None);
        let mut mismatch = form.clone();
        mismatch.kind = Kind::Symbol(suss_reader::Symbol::new("other"));
        assert_eq!(
            origin
                .symbol_position_metered(&mismatch, &mut |_| Ok(()))
                .unwrap(),
            None
        );

        let mut invalid_span = form;
        invalid_span.span = 10..20;
        assert_eq!(
            origin
                .symbol_position_metered(&invalid_span, &mut |_| Ok(()))
                .unwrap(),
            None
        );
    }

    #[test]
    fn metered_symbol_position_propagates_cancellation_and_recovers() {
        let source = "^{:tag true} ns/name";
        let form = read_forms(source).unwrap().remove(0);
        let origin = SourceOrigin::new(source, None);
        let mut remaining = 1u64;
        let error = origin
            .symbol_position_metered(&form, &mut |work| {
                remaining =
                    remaining
                        .checked_sub(work)
                        .ok_or_else(|| super::super::Diagnostic {
                            span: 80..81,
                            message: "symbol position cancelled".into(),
                        })?;
                Ok(())
            })
            .unwrap_err();
        assert_eq!(error.span, 80..81);
        assert_eq!(error.message, "symbol position cancelled");
        assert_eq!(
            origin
                .symbol_position_metered(&form, &mut |_| Ok(()))
                .unwrap(),
            Some(SourcePosition {
                line: 1,
                column: 14
            })
        );
    }
}
