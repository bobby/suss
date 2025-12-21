//! String interning for symbols and keywords
//!
//! Provides O(1) equality comparison for symbols and keywords.

use std::collections::HashMap;

/// Interned symbol identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SymbolId(pub u32);

/// Interned keyword identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeywordId(pub u32);

/// String interner for symbols and keywords
#[derive(Debug, Default, Clone)]
pub struct Interner {
    symbols: Vec<String>,
    symbol_map: HashMap<String, SymbolId>,
    keywords: Vec<String>,
    keyword_map: HashMap<String, KeywordId>,
}

impl Interner {
    /// Create a new interner
    pub fn new() -> Self {
        Self::default()
    }

    /// Intern a symbol, returning its ID
    pub fn intern_symbol(&mut self, name: &str) -> SymbolId {
        if let Some(&id) = self.symbol_map.get(name) {
            return id;
        }
        let id = SymbolId(self.symbols.len() as u32);
        self.symbols.push(name.to_string());
        self.symbol_map.insert(name.to_string(), id);
        id
    }

    /// Get the name of an interned symbol
    pub fn symbol_name(&self, id: SymbolId) -> &str {
        &self.symbols[id.0 as usize]
    }

    /// Intern a keyword (without the leading colon), returning its ID
    pub fn intern_keyword(&mut self, name: &str) -> KeywordId {
        if let Some(&id) = self.keyword_map.get(name) {
            return id;
        }
        let id = KeywordId(self.keywords.len() as u32);
        self.keywords.push(name.to_string());
        self.keyword_map.insert(name.to_string(), id);
        id
    }

    /// Get the name of an interned keyword (without the leading colon)
    pub fn keyword_name(&self, id: KeywordId) -> &str {
        &self.keywords[id.0 as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_interning() {
        let mut interner = Interner::new();
        let foo1 = interner.intern_symbol("foo");
        let bar = interner.intern_symbol("bar");
        let foo2 = interner.intern_symbol("foo");

        assert_eq!(foo1, foo2);
        assert_ne!(foo1, bar);
        assert_eq!(interner.symbol_name(foo1), "foo");
        assert_eq!(interner.symbol_name(bar), "bar");
    }

    #[test]
    fn test_keyword_interning() {
        let mut interner = Interner::new();
        let foo1 = interner.intern_keyword("foo");
        let bar = interner.intern_keyword("bar");
        let foo2 = interner.intern_keyword("foo");

        assert_eq!(foo1, foo2);
        assert_ne!(foo1, bar);
        assert_eq!(interner.keyword_name(foo1), "foo");
        assert_eq!(interner.keyword_name(bar), "bar");
    }
}
