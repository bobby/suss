// Copyright (c) Rich Hickey. All rights reserved.
// The use and distribution terms for this software are covered by the
// Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
// which can be found in clojurescript/epl-v10.html in this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.
// You must not remove this notice, or any other, from this software.
//! Original source-function display-name adaptation.
//! Behavior provenance: pinned ClojureScript c4295f303100bbf5afac449242d30bca1126f1a1,
//! compiler.cljc fn-self-name/munge (98–148), analyzer.cljc fn-name-var (2279–2298),
//! and core.cljs CHAR_MAP (378–404). Upstream is EPL-1.0; see clojurescript/epl-v10.html.
//! Names use actual source scopes, never invented lexical binding identifiers.
use super::hir::Hir;
use suss_reader::forms::Kind;

pub(super) fn display_name(hir: &Hir) -> Option<Vec<u16>> {
    let callable = hir.source.as_ref()?.callable.as_ref()?;
    let Some(scope) = callable.name.as_ref() else {
        return Some(Vec::new());
    };
    let spelling = |form: &suss_reader::forms::Form| match &form.kind {
        Kind::Symbol(symbol) => match &symbol.namespace {
            Some(namespace) => format!("{namespace}/{}", symbol.name),
            None => symbol.name.clone(),
        },
        _ => unreachable!("source function declaration is a symbol"),
    };
    let mut names: Vec<String> = scope
        .parents
        .iter()
        .map(|parent| spelling(&parent.declaration))
        .collect();
    names.push(spelling(&scope.declaration).replace("..", "_DOT__DOT_"));
    // The imported portable core uses an internal storage namespace; cljs.core
    // is its canonical public source identity, as in resolution and syntax quote.
    let namespace = if scope.namespace == "suss.core" {
        "cljs.core"
    } else {
        &scope.namespace
    };
    Some(
        munge(&format!(
            "{}${}",
            namespace.replace('.', "$"),
            names.join("_$_")
        ))
        .encode_utf16()
        .collect(),
    )
}

fn munge(source: &str) -> String {
    let replaced = source.replace("..", "_DOT__DOT_");
    let mut slash = String::new();
    let mut chars = replaced.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '/' && chars.peek().is_some() {
            slash.push('.');
        } else {
            slash.push(c);
        }
    }
    let reserved = [
        "arguments",
        "abstract",
        "await",
        "boolean",
        "break",
        "byte",
        "case",
        "catch",
        "char",
        "class",
        "const",
        "continue",
        "debugger",
        "default",
        "delete",
        "do",
        "double",
        "else",
        "enum",
        "export",
        "extends",
        "final",
        "finally",
        "float",
        "for",
        "function",
        "goto",
        "if",
        "implements",
        "import",
        "in",
        "instanceof",
        "int",
        "interface",
        "let",
        "long",
        "native",
        "new",
        "package",
        "private",
        "protected",
        "public",
        "return",
        "short",
        "static",
        "super",
        "switch",
        "synchronized",
        "this",
        "throw",
        "throws",
        "transient",
        "try",
        "typeof",
        "var",
        "void",
        "volatile",
        "while",
        "with",
        "yield",
        "methods",
        "null",
        "constructor",
    ];
    let escaped = slash
        .split('.')
        .map(|part| {
            if reserved.contains(&part) {
                format!("{part}$")
            } else {
                part.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(".");
    let mut output = String::new();
    for c in escaped.chars() {
        let replacement = match c {
            '-' => "_",
            ':' => "_COLON_",
            '+' => "_PLUS_",
            '>' => "_GT_",
            '<' => "_LT_",
            '=' => "_EQ_",
            '~' => "_TILDE_",
            '!' => "_BANG_",
            '@' => "_CIRCA_",
            '#' => "_SHARP_",
            '\'' => "_SINGLEQUOTE_",
            '"' => "_DOUBLEQUOTE_",
            '%' => "_PERCENT_",
            '^' => "_CARET_",
            '&' => "_AMPERSAND_",
            '*' => "_STAR_",
            '|' => "_BAR_",
            '{' => "_LBRACE_",
            '}' => "_RBRACE_",
            '[' => "_LBRACK_",
            ']' => "_RBRACK_",
            '/' => "_SLASH_",
            '\\' => "_BSLASH_",
            '?' => "_QMARK_",
            _ => {
                output.push(c);
                continue;
            }
        };
        output.push_str(replacement);
    }
    output
}
