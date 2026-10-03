//! Immutable source provenance for compiler and compiled macro facts.
use std::{
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};
mod form_metadata;

#[derive(Debug, Clone)]
pub struct SourceOrigin {
    text: Arc<str>,
    path: Option<PathBuf>,
    line_starts: Arc<[usize]>,
    metadata_forms: Arc<
        OnceLock<Result<(Vec<suss_reader::forms::Form>, Vec<suss_reader::forms::Form>), String>>,
    >,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourcePosition {
    pub line: usize,
    /// One-based UTF-16 code-unit column, independent of UTF-8 byte width.
    pub column: usize,
}
impl SourceOrigin {
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
        use suss_reader::forms::{read_forms, resolve_conditionals, Kind};
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
