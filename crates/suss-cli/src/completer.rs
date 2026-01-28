//! Tab completion for the Suss REPL
//!
//! Provides symbol completion using rustyline's completion API.

use std::sync::{Arc, RwLock};
use std::collections::HashMap;

use rustyline::completion::{Completer, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::Helper;
use rustyline::Context;

use crate::session::{SymbolEntry, SymbolKind};

/// Tab completion helper for the Suss REPL
pub struct SussCompleter {
    /// Shared symbol table (updated as definitions are added)
    symbols: Arc<RwLock<HashMap<String, SymbolEntry>>>,
}

impl SussCompleter {
    /// Create a new completer with a shared symbol table
    pub fn new(symbols: Arc<RwLock<HashMap<String, SymbolEntry>>>) -> Self {
        Self { symbols }
    }

    /// Update the symbol table reference
    pub fn update_symbols(&mut self, symbols: Arc<RwLock<HashMap<String, SymbolEntry>>>) {
        self.symbols = symbols;
    }
}

impl Completer for SussCompleter {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        // Find the start of the current symbol
        // Symbols can start after: whitespace, (, [, {, ', `
        let start = line[..pos]
            .rfind(|c: char| {
                c.is_whitespace() || c == '(' || c == '[' || c == '{' || c == '\'' || c == '`'
            })
            .map(|i| i + 1)
            .unwrap_or(0);

        let prefix = &line[start..pos];

        // Don't complete empty prefix or very short prefixes
        if prefix.is_empty() {
            return Ok((start, Vec::new()));
        }

        // Match against symbol table
        let symbols = self.symbols.read().unwrap();
        let mut matches: Vec<Pair> = symbols
            .iter()
            .filter(|(name, _)| name.starts_with(prefix))
            .map(|(name, entry)| {
                let display = format_completion(name, entry);
                Pair {
                    display,
                    replacement: name.clone(),
                }
            })
            .collect();

        // Sort alphabetically
        matches.sort_by(|a, b| a.replacement.cmp(&b.replacement));

        // Limit results
        if matches.len() > 50 {
            matches.truncate(50);
        }

        Ok((start, matches))
    }
}

/// Format a completion entry for display
fn format_completion(name: &str, entry: &SymbolEntry) -> String {
    let kind_str = match entry.kind {
        SymbolKind::Function => "fn",
        SymbolKind::Macro => "macro",
        SymbolKind::Var => "var",
        SymbolKind::Protocol => "protocol",
        SymbolKind::Type => "type",
        SymbolKind::Special => "special",
    };

    if let Some(arity) = entry.arity {
        format!("{}  [{}/{}]", name, kind_str, arity)
    } else {
        format!("{}  [{}]", name, kind_str)
    }
}

// Implement required traits for Helper
impl Hinter for SussCompleter {
    type Hint = String;

    fn hint(&self, _line: &str, _pos: usize, _ctx: &Context<'_>) -> Option<String> {
        // No inline hints for now
        None
    }
}

impl Highlighter for SussCompleter {
    // Default implementation - no highlighting
}

impl Validator for SussCompleter {
    // Default implementation - always valid
}

impl Helper for SussCompleter {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_completion() {
        let entry = SymbolEntry {
            name: "map".to_string(),
            kind: SymbolKind::Function,
            namespace: None,
            arity: Some(2),
        };
        assert_eq!(format_completion("map", &entry), "map  [fn/2]");

        let entry = SymbolEntry {
            name: "when".to_string(),
            kind: SymbolKind::Macro,
            namespace: None,
            arity: None,
        };
        assert_eq!(format_completion("when", &entry), "when  [macro]");
    }
}
