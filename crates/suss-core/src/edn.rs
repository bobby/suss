//! EDN (Extensible Data Notation) types for Suss
//!
//! This module defines the core value types for Suss, based on EDN/Clojure semantics.
//! All values implement Display with read-print isomorphism: printed values can be read back.

use crate::Number;
use std::fmt;

/// Symbol with optional namespace (e.g., foo, bar/baz)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Symbol {
    pub namespace: Option<String>,
    pub name: String,
}

impl Symbol {
    /// Create a simple symbol without namespace
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            namespace: None,
            name: name.into(),
        }
    }

    /// Create a namespaced symbol
    pub fn namespaced(namespace: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            namespace: Some(namespace.into()),
            name: name.into(),
        }
    }

    /// Parse a symbol from a string (e.g., "foo" or "bar/baz")
    pub fn parse(s: &str) -> Self {
        // Special case: "/" is the division symbol, not namespaced
        if s == "/" {
            return Self::new(s);
        }
        if let Some(idx) = s.find('/') {
            let (ns, name) = s.split_at(idx);
            Self {
                namespace: Some(ns.to_string()),
                name: name[1..].to_string(), // skip the '/'
            }
        } else {
            Self::new(s)
        }
    }
}

impl fmt::Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ns) = &self.namespace {
            write!(f, "{}/{}", ns, self.name)
        } else {
            write!(f, "{}", self.name)
        }
    }
}

/// Keyword with optional namespace (e.g., :foo, :bar/baz)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Keyword {
    pub namespace: Option<String>,
    pub name: String,
}

impl Keyword {
    /// Create a simple keyword without namespace
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            namespace: None,
            name: name.into(),
        }
    }

    /// Create a namespaced keyword
    pub fn namespaced(namespace: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            namespace: Some(namespace.into()),
            name: name.into(),
        }
    }

    /// Parse a keyword from a string (without the leading colon)
    pub fn parse(s: &str) -> Self {
        if let Some(idx) = s.find('/') {
            let (ns, name) = s.split_at(idx);
            Self {
                namespace: Some(ns.to_string()),
                name: name[1..].to_string(),
            }
        } else {
            Self::new(s)
        }
    }
}

impl fmt::Display for Keyword {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ns) = &self.namespace {
            write!(f, ":{}/{}", ns, self.name)
        } else {
            write!(f, ":{}", self.name)
        }
    }
}

/// Tagged literal (e.g., #inst "2024-01-01", #uuid "...", #my/tag {...})
#[derive(Debug, Clone, PartialEq)]
pub struct Tagged {
    pub tag: Symbol,
    pub value: Box<Edn>,
}

impl Tagged {
    pub fn new(tag: Symbol, value: Edn) -> Self {
        Self {
            tag,
            value: Box::new(value),
        }
    }
}

impl fmt::Display for Tagged {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{} {}", self.tag, self.value)
    }
}

/// EDN value - the core data type for Suss/Clojure
///
/// All variants (except Primitive and Function) support read-print isomorphism:
/// `read(edn.to_string()) == edn`
#[derive(Debug, Clone)]
pub enum Edn {
    /// nil - the empty/null value
    Nil,
    /// Boolean true or false
    Bool(bool),
    /// Number (integer, ratio, or float)
    Number(Number),
    /// Unicode character (e.g., \a, \newline)
    Char(char),
    /// String literal
    String(String),
    /// Symbol (e.g., foo, bar/baz)
    Symbol(Symbol),
    /// Keyword (e.g., :foo, :bar/baz)
    Keyword(Keyword),
    /// List - linked list semantics (...)
    List(Vec<Edn>),
    /// Vector - indexed access [...]
    Vector(Vec<Edn>),
    /// Set - unique elements #{...}
    Set(Vec<Edn>),
    /// Map - key-value pairs {...}
    Map(Vec<(Edn, Edn)>),
    /// Tagged literal - extensible data notation
    Tagged(Tagged),
    /// Reader conditional #?(:platform expr ...)
    ReaderConditional(Vec<(Keyword, Edn)>),
    /// Built-in primitive function (runtime only, not serializable)
    Primitive(String),
    /// User-defined function/closure (runtime only, not serializable)
    Function {
        params: Vec<Symbol>,
        body: Box<Edn>,
        env: Box<crate::Env>,
    },
}

impl PartialEq for Edn {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Edn::Nil, Edn::Nil) => true,
            (Edn::Bool(a), Edn::Bool(b)) => a == b,
            (Edn::Number(a), Edn::Number(b)) => a == b,
            (Edn::Char(a), Edn::Char(b)) => a == b,
            (Edn::String(a), Edn::String(b)) => a == b,
            (Edn::Symbol(a), Edn::Symbol(b)) => a == b,
            (Edn::Keyword(a), Edn::Keyword(b)) => a == b,
            (Edn::List(a), Edn::List(b)) => a == b,
            (Edn::Vector(a), Edn::Vector(b)) => a == b,
            (Edn::Set(a), Edn::Set(b)) => a == b,
            (Edn::Map(a), Edn::Map(b)) => a == b,
            (Edn::Tagged(a), Edn::Tagged(b)) => a == b,
            (Edn::ReaderConditional(a), Edn::ReaderConditional(b)) => a == b,
            (Edn::Primitive(a), Edn::Primitive(b)) => a == b,
            // Functions are never equal (like Clojure)
            (Edn::Function { .. }, Edn::Function { .. }) => false,
            _ => false,
        }
    }
}

impl Edn {
    /// Check if this is nil
    pub fn is_nil(&self) -> bool {
        matches!(self, Edn::Nil)
    }

    /// Check if this is a truthy value (not nil or false)
    pub fn is_truthy(&self) -> bool {
        !matches!(self, Edn::Nil | Edn::Bool(false))
    }

    /// Get as a symbol if this is a symbol
    pub fn as_symbol(&self) -> Option<&Symbol> {
        match self {
            Edn::Symbol(s) => Some(s),
            _ => None,
        }
    }

    /// Get as a keyword if this is a keyword
    pub fn as_keyword(&self) -> Option<&Keyword> {
        match self {
            Edn::Keyword(k) => Some(k),
            _ => None,
        }
    }

    /// Get as a list if this is a list
    pub fn as_list(&self) -> Option<&[Edn]> {
        match self {
            Edn::List(items) => Some(items),
            _ => None,
        }
    }

    /// Get as a vector if this is a vector
    pub fn as_vector(&self) -> Option<&[Edn]> {
        match self {
            Edn::Vector(items) => Some(items),
            _ => None,
        }
    }
}

impl fmt::Display for Edn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Edn::Nil => write!(f, "nil"),
            Edn::Bool(true) => write!(f, "true"),
            Edn::Bool(false) => write!(f, "false"),
            Edn::Number(n) => write!(f, "{}", n),
            Edn::Char(c) => write_char(f, *c),
            Edn::String(s) => write_string(f, s),
            Edn::Symbol(sym) => write!(f, "{}", sym),
            Edn::Keyword(kw) => write!(f, "{}", kw),
            Edn::List(items) => write_seq(f, "(", items, ")"),
            Edn::Vector(items) => write_seq(f, "[", items, "]"),
            Edn::Set(items) => write_seq(f, "#{", items, "}"),
            Edn::Map(pairs) => write_map(f, pairs),
            Edn::Tagged(t) => write!(f, "{}", t),
            Edn::ReaderConditional(clauses) => {
                write!(f, "#?(")?;
                for (i, (platform, expr)) in clauses.iter().enumerate() {
                    if i > 0 {
                        write!(f, " ")?;
                    }
                    write!(f, "{} {}", platform, expr)?;
                }
                write!(f, ")")
            }
            Edn::Primitive(name) => write!(f, "#<primitive:{}>", name),
            Edn::Function { .. } => write!(f, "#<function>"),
        }
    }
}

/// Write a character in EDN format
fn write_char(f: &mut fmt::Formatter<'_>, c: char) -> fmt::Result {
    match c {
        '\n' => write!(f, "\\newline"),
        '\r' => write!(f, "\\return"),
        '\t' => write!(f, "\\tab"),
        ' ' => write!(f, "\\space"),
        '\\' => write!(f, "\\\\"),
        _ => write!(f, "\\{}", c),
    }
}

/// Write a string in EDN format with proper escaping
fn write_string(f: &mut fmt::Formatter<'_>, s: &str) -> fmt::Result {
    write!(f, "\"")?;
    for c in s.chars() {
        match c {
            '"' => write!(f, "\\\"")?,
            '\\' => write!(f, "\\\\")?,
            '\n' => write!(f, "\\n")?,
            '\r' => write!(f, "\\r")?,
            '\t' => write!(f, "\\t")?,
            _ => write!(f, "{}", c)?,
        }
    }
    write!(f, "\"")
}

/// Write a sequence (list, vector, or set) in EDN format
fn write_seq(f: &mut fmt::Formatter<'_>, open: &str, items: &[Edn], close: &str) -> fmt::Result {
    write!(f, "{}", open)?;
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            write!(f, " ")?;
        }
        write!(f, "{}", item)?;
    }
    write!(f, "{}", close)
}

/// Write a map in EDN format
fn write_map(f: &mut fmt::Formatter<'_>, pairs: &[(Edn, Edn)]) -> fmt::Result {
    write!(f, "{{")?;
    for (i, (k, v)) in pairs.iter().enumerate() {
        if i > 0 {
            write!(f, ", ")?;
        }
        write!(f, "{} {}", k, v)?;
    }
    write!(f, "}}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_display() {
        assert_eq!(Symbol::new("foo").to_string(), "foo");
        assert_eq!(Symbol::namespaced("bar", "baz").to_string(), "bar/baz");
    }

    #[test]
    fn test_symbol_with_bang() {
        // Test Symbol::parse with bang symbols
        let sym = Symbol::parse("persistent!");
        assert_eq!(sym.namespace, None);
        assert_eq!(sym.name, "persistent!");

        let sym2 = Symbol::parse("suss.core/persistent!");
        assert_eq!(sym2.namespace, Some("suss.core".to_string()));
        assert_eq!(sym2.name, "persistent!");

        let sym3 = Symbol::parse("conj!");
        assert_eq!(sym3.name, "conj!");
    }

    #[test]
    fn test_keyword_display() {
        assert_eq!(Keyword::new("foo").to_string(), ":foo");
        assert_eq!(Keyword::namespaced("bar", "baz").to_string(), ":bar/baz");
    }

    #[test]
    fn test_edn_display() {
        assert_eq!(Edn::Nil.to_string(), "nil");
        assert_eq!(Edn::Bool(true).to_string(), "true");
        assert_eq!(Edn::Bool(false).to_string(), "false");
        assert_eq!(Edn::String("hello".to_string()).to_string(), "\"hello\"");
        assert_eq!(Edn::Char('\n').to_string(), "\\newline");
    }

    #[test]
    fn test_collection_display() {
        let list = Edn::List(vec![
            Edn::Number(Number::Integer(1.into())),
            Edn::Number(Number::Integer(2.into())),
        ]);
        assert_eq!(list.to_string(), "(1 2)");

        let vec = Edn::Vector(vec![
            Edn::Keyword(Keyword::new("a")),
            Edn::Keyword(Keyword::new("b")),
        ]);
        assert_eq!(vec.to_string(), "[:a :b]");
    }
}
