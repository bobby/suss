//! S-expression type definition
//!
//! The Sexp type represents all Suss values.

use crate::{Env, Interner, KeywordId, Number, SymbolId};

/// An S-expression - the core Suss data type
#[derive(Debug, Clone)]
pub enum Sexp {
    /// nil - the empty/null value
    Nil,
    /// Boolean true or false
    Bool(bool),
    /// Interned symbol
    Symbol(SymbolId),
    /// Interned keyword (like :foo)
    Keyword(KeywordId),
    /// Number (integer, ratio, or float)
    Number(Number),
    /// Character literal
    Char(char),
    /// String literal
    String(String),
    /// List (linked list semantics)
    List(Vec<Sexp>),
    /// Vector (indexed access)
    Vector(Vec<Sexp>),
    /// Set #{...}
    Set(Vec<Sexp>),
    /// Map {...}
    Map(Vec<(Sexp, Sexp)>),
    /// Reader conditional #?(:platform expr ...)
    ReaderConditional(Vec<(KeywordId, Sexp)>),
    /// Built-in primitive function
    Primitive(String),
    /// User-defined function (closure)
    Function {
        params: Vec<SymbolId>,
        body: Box<Sexp>,
        env: Box<Env>,
    },
}

impl PartialEq for Sexp {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Sexp::Nil, Sexp::Nil) => true,
            (Sexp::Bool(a), Sexp::Bool(b)) => a == b,
            (Sexp::Symbol(a), Sexp::Symbol(b)) => a == b,
            (Sexp::Keyword(a), Sexp::Keyword(b)) => a == b,
            (Sexp::Number(a), Sexp::Number(b)) => a == b,
            (Sexp::Char(a), Sexp::Char(b)) => a == b,
            (Sexp::String(a), Sexp::String(b)) => a == b,
            (Sexp::List(a), Sexp::List(b)) => a == b,
            (Sexp::Vector(a), Sexp::Vector(b)) => a == b,
            (Sexp::Set(a), Sexp::Set(b)) => a == b,
            (Sexp::Map(a), Sexp::Map(b)) => a == b,
            (Sexp::ReaderConditional(a), Sexp::ReaderConditional(b)) => a == b,
            (Sexp::Primitive(a), Sexp::Primitive(b)) => a == b,
            // Functions are never equal (like Clojure)
            (Sexp::Function { .. }, Sexp::Function { .. }) => false,
            _ => false,
        }
    }
}

impl Sexp {
    /// Check if this is nil
    pub fn is_nil(&self) -> bool {
        matches!(self, Sexp::Nil)
    }

    /// Check if this is a truthy value (not nil or false)
    pub fn is_truthy(&self) -> bool {
        !matches!(self, Sexp::Nil | Sexp::Bool(false))
    }

    /// Get as a symbol ID if this is a symbol
    pub fn as_symbol(&self) -> Option<SymbolId> {
        match self {
            Sexp::Symbol(id) => Some(*id),
            _ => None,
        }
    }

    /// Get as a keyword ID if this is a keyword
    pub fn as_keyword(&self) -> Option<KeywordId> {
        match self {
            Sexp::Keyword(id) => Some(*id),
            _ => None,
        }
    }

    /// Get as a list if this is a list
    pub fn as_list(&self) -> Option<&[Sexp]> {
        match self {
            Sexp::List(items) => Some(items),
            _ => None,
        }
    }

    /// Get as a vector if this is a vector
    pub fn as_vector(&self) -> Option<&[Sexp]> {
        match self {
            Sexp::Vector(items) => Some(items),
            _ => None,
        }
    }
}

/// Print an s-expression using the given interner for symbol/keyword names
pub fn print_sexp(sexp: &Sexp, interner: &Interner) -> String {
    fn write(sexp: &Sexp, interner: &Interner, out: &mut String) {
        match sexp {
            Sexp::Nil => out.push_str("nil"),
            Sexp::Bool(true) => out.push_str("true"),
            Sexp::Bool(false) => out.push_str("false"),
            Sexp::Symbol(id) => out.push_str(interner.symbol_name(*id)),
            Sexp::Keyword(id) => {
                out.push(':');
                out.push_str(interner.keyword_name(*id));
            }
            Sexp::Number(n) => out.push_str(&n.to_string()),
            Sexp::Char(c) => match c {
                '\n' => out.push_str("\\newline"),
                '\r' => out.push_str("\\return"),
                '\t' => out.push_str("\\tab"),
                ' ' => out.push_str("\\space"),
                _ => {
                    out.push('\\');
                    out.push(*c);
                }
            },
            Sexp::String(s) => {
                out.push('"');
                for c in s.chars() {
                    match c {
                        '"' => out.push_str("\\\""),
                        '\\' => out.push_str("\\\\"),
                        '\n' => out.push_str("\\n"),
                        '\r' => out.push_str("\\r"),
                        '\t' => out.push_str("\\t"),
                        _ => out.push(c),
                    }
                }
                out.push('"');
            }
            Sexp::List(items) => {
                out.push('(');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(' ');
                    }
                    write(item, interner, out);
                }
                out.push(')');
            }
            Sexp::Vector(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(' ');
                    }
                    write(item, interner, out);
                }
                out.push(']');
            }
            Sexp::Set(items) => {
                out.push_str("#{");
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(' ');
                    }
                    write(item, interner, out);
                }
                out.push('}');
            }
            Sexp::Map(pairs) => {
                out.push('{');
                for (i, (k, v)) in pairs.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    write(k, interner, out);
                    out.push(' ');
                    write(v, interner, out);
                }
                out.push('}');
            }
            Sexp::ReaderConditional(clauses) => {
                out.push_str("#?(");
                for (i, (platform, expr)) in clauses.iter().enumerate() {
                    if i > 0 {
                        out.push(' ');
                    }
                    out.push(':');
                    out.push_str(interner.keyword_name(*platform));
                    out.push(' ');
                    write(expr, interner, out);
                }
                out.push(')');
            }
            Sexp::Primitive(name) => {
                out.push_str("#<primitive:");
                out.push_str(name);
                out.push('>');
            }
            Sexp::Function { .. } => {
                out.push_str("#<function>");
            }
        }
    }

    let mut result = String::new();
    write(sexp, interner, &mut result);
    result
}
