//! Chumsky-based s-expression parser
//!
//! Provides a full EDN parser with error recovery for REPL use.

use chumsky::prelude::*;
use suss_core::{Edn, Keyword, Number, Symbol};

/// A parse error with source location and recovery hints
#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub span: std::ops::Range<usize>,
    pub expected: Vec<String>,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ParseError {}

/// Parser state (simplified - no longer needs interner)
pub struct ParserState {
    pub platform: String,
}

impl ParserState {
    pub fn new(platform: impl Into<String>) -> Self {
        Self {
            platform: platform.into(),
        }
    }
}

/// Parse a single s-expression from a string
pub fn parse(input: &str, state: &mut ParserState) -> Result<Edn, ParseError> {
    let (edn, errors) = parse_inner(input, state);

    if let Some(edn) = edn {
        if errors.is_empty() {
            return Ok(edn);
        }
    }

    let err = errors.into_iter().next().unwrap_or_else(|| ParseError {
        message: "Unexpected end of input".to_string(),
        span: input.len()..input.len(),
        expected: vec![],
    });

    Err(err)
}

/// Parse all s-expressions from a string
pub fn parse_all(input: &str, state: &mut ParserState) -> Result<Vec<Edn>, ParseError> {
    let (edns, errors) = parse_all_inner(input, state);

    if errors.is_empty() {
        Ok(edns)
    } else {
        let err = errors.into_iter().next().unwrap();
        Err(err)
    }
}

fn parse_inner(input: &str, state: &mut ParserState) -> (Option<Edn>, Vec<ParseError>) {
    let parser = edn_parser(state);
    let (result, errors) = parser.parse(input).into_output_errors();

    let parse_errors: Vec<ParseError> = errors
        .into_iter()
        .map(|e| ParseError {
            message: e.to_string(),
            span: e.span().into_range(),
            expected: e
                .expected()
                .map(|exp| format!("{:?}", exp))
                .collect(),
        })
        .collect();

    (result, parse_errors)
}

fn parse_all_inner(input: &str, state: &mut ParserState) -> (Vec<Edn>, Vec<ParseError>) {
    let parser = edn_parser(state).repeated().collect::<Vec<_>>();
    let (result, errors) = parser.parse(input).into_output_errors();

    let parse_errors: Vec<ParseError> = errors
        .into_iter()
        .map(|e| ParseError {
            message: e.to_string(),
            span: e.span().into_range(),
            expected: e
                .expected()
                .map(|exp| format!("{:?}", exp))
                .collect(),
        })
        .collect();

    (result.unwrap_or_default(), parse_errors)
}

/// Build the EDN parser
fn edn_parser<'a>(_state: &'a mut ParserState) -> impl Parser<'a, &'a str, Edn, extra::Err<Rich<'a, char>>> {
    recursive(|edn| {
        // Whitespace characters (at least one)
        let ws = one_of(" \t\n\r,").repeated().at_least(1);

        // Comments: ; to end of line
        let comment = just(';')
            .then(any().and_is(just('\n').not()).repeated())
            .then(just('\n').or_not());

        // A single whitespace item (either whitespace or a comment)
        let ws_item = ws.to(()).or(comment.to(()));

        // Zero or more whitespace items
        let padding = ws_item.repeated();

        // nil
        let nil = text::keyword("nil").to(Edn::Nil);

        // Booleans
        let boolean = choice((
            text::keyword("true").to(Edn::Bool(true)),
            text::keyword("false").to(Edn::Bool(false)),
        ));

        // Character literals
        let character = just('\\').ignore_then(choice((
            text::keyword("newline").to('\n'),
            text::keyword("return").to('\r'),
            text::keyword("space").to(' '),
            text::keyword("tab").to('\t'),
            any(),
        )))
        .map(Edn::Char);

        // String literals
        let escape = just('\\').ignore_then(choice((
            just('n').to('\n'),
            just('r').to('\r'),
            just('t').to('\t'),
            just('\\').to('\\'),
            just('"').to('"'),
        )));

        let string_char = none_of("\\\"").or(escape);
        let string = string_char
            .repeated()
            .collect::<String>()
            .delimited_by(just('"'), just('"'))
            .map(Edn::String);

        // Symbol characters
        let symbol_start = any().filter(|c: &char| {
            c.is_alphabetic()
                || matches!(c, '*' | '+' | '!' | '-' | '_' | '?' | '<' | '>' | '=' | '&' | '.' | '/' | '^')
        });

        let symbol_char = any().filter(|c: &char| {
            c.is_alphanumeric()
                || matches!(c, '*' | '+' | '!' | '-' | '_' | '?' | '\'' | '<' | '>' | '=' | '&' | '.' | '/' | ':' | '#')
        });

        // Numbers - must come before symbols to catch negative numbers
        let sign = one_of("+-").or_not();
        let digits = text::digits(10);

        // Float: 3.14, -2.5e10
        let float = sign
            .then(digits.clone())
            .then(just('.'))
            .then(digits.clone())
            .then(
                one_of("eE")
                    .then(sign)
                    .then(digits.clone())
                    .or_not(),
            )
            .to_slice()
            .try_map(|s: &str, span| {
                Number::parse_float(s)
                    .ok_or_else(|| Rich::custom(span, format!("Invalid float: {}", s)))
            })
            .map(Edn::Number);

        // Special floats
        let special_float = choice((
            just("##Inf").to(Edn::Number(Number::Float(f64::INFINITY))),
            just("##-Inf").to(Edn::Number(Number::Float(f64::NEG_INFINITY))),
            just("##NaN").to(Edn::Number(Number::Float(f64::NAN))),
        ));

        // Ratio: 1/3, -5/7
        let ratio = sign
            .then(digits.clone())
            .then(just('/'))
            .then(digits.clone())
            .to_slice()
            .try_map(|s: &str, span| {
                Number::parse_ratio(s)
                    .ok_or_else(|| Rich::custom(span, format!("Invalid ratio: {}", s)))
            })
            .map(Edn::Number);

        // Integer: 42, -17, 0xFF, 2r1010
        let hex_int = just("0x")
            .or(just("0X"))
            .then(text::digits(16))
            .to_slice();

        let oct_int = just("0o")
            .or(just("0O"))
            .then(text::digits(8))
            .to_slice();

        let bin_int = just("0b")
            .or(just("0B"))
            .then(text::digits(2))
            .to_slice();

        let radix_int = text::digits(10)
            .then(just('r').or(just('R')))
            .then(text::digits(36))
            .to_slice();

        let dec_int = sign.then(digits).to_slice();

        let integer = choice((hex_int, oct_int, bin_int, radix_int, dec_int))
            .try_map(|s: &str, span| {
                Number::parse_integer(s)
                    .ok_or_else(|| Rich::custom(span, format!("Invalid integer: {}", s)))
            })
            .map(Edn::Number);

        // Number: try floats and ratios before integers
        let number = choice((special_float, float, ratio, integer));

        // Keyword: :foo, :bar/baz
        let keyword = just(':')
            .ignore_then(
                symbol_start
                    .then(symbol_char.repeated())
                    .to_slice(),
            )
            .map(|name: &str| Edn::Keyword(Keyword::parse(name)));

        // Symbol: foo, bar/baz
        let symbol = symbol_start
            .then(symbol_char.repeated())
            .to_slice()
            .map(|name: &str| Edn::Symbol(Symbol::parse(name)));

        // List: (a b c)
        let list = edn
            .clone()
            .padded_by(padding.clone())
            .repeated()
            .collect::<Vec<_>>()
            .delimited_by(just('('), just(')'))
            .map(Edn::List);

        // Vector: [a b c]
        let vector = edn
            .clone()
            .padded_by(padding.clone())
            .repeated()
            .collect::<Vec<_>>()
            .delimited_by(just('['), just(']'))
            .map(Edn::Vector);

        // Set: #{a b c}
        let set = edn
            .clone()
            .padded_by(padding.clone())
            .repeated()
            .collect::<Vec<_>>()
            .delimited_by(just("#{"), just('}'))
            .map(Edn::Set);

        // Map: {k v k v}
        let map_pair = edn
            .clone()
            .padded_by(padding.clone())
            .then(edn.clone().padded_by(padding.clone()));

        let map = map_pair
            .repeated()
            .collect::<Vec<_>>()
            .delimited_by(just('{'), just('}'))
            .map(Edn::Map);

        // Quote: 'x -> (quote x)
        let quote = just('\'')
            .ignore_then(edn.clone())
            .map(|x| Edn::List(vec![Edn::Symbol(Symbol::new("quote")), x]));

        // Syntax quote, unquote, etc.
        let syntax_quote = just('`')
            .ignore_then(edn.clone())
            .map(|x| Edn::List(vec![Edn::Symbol(Symbol::new("syntax-quote")), x]));

        let unquote_splice = just("~@")
            .ignore_then(edn.clone())
            .map(|x| Edn::List(vec![Edn::Symbol(Symbol::new("unquote-splicing")), x]));

        let unquote = just('~')
            .ignore_then(edn.clone())
            .map(|x| Edn::List(vec![Edn::Symbol(Symbol::new("unquote")), x]));

        let deref = just('@')
            .ignore_then(edn.clone())
            .map(|x| Edn::List(vec![Edn::Symbol(Symbol::new("deref")), x]));

        // All atoms and compounds
        choice((
            nil,
            boolean,
            character,
            string,
            number,
            keyword,
            quote,
            syntax_quote,
            unquote_splice,
            unquote,
            deref,
            list,
            vector,
            set,
            map,
            symbol, // Symbol must come last as it's very permissive
        ))
        .padded_by(padding)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_nil() {
        let mut state = ParserState::new("suss");
        let result = parse("nil", &mut state).unwrap();
        assert_eq!(result, Edn::Nil);
    }

    #[test]
    fn test_parse_bool() {
        let mut state = ParserState::new("suss");
        assert_eq!(parse("true", &mut state).unwrap(), Edn::Bool(true));
        assert_eq!(parse("false", &mut state).unwrap(), Edn::Bool(false));
    }

    #[test]
    fn test_parse_string() {
        let mut state = ParserState::new("suss");
        assert_eq!(
            parse("\"hello\"", &mut state).unwrap(),
            Edn::String("hello".to_string())
        );
    }

    #[test]
    fn test_parse_number() {
        let mut state = ParserState::new("suss");
        if let Edn::Number(Number::Integer(n)) = parse("42", &mut state).unwrap() {
            assert_eq!(n.to_string(), "42");
        } else {
            panic!("Expected integer");
        }
    }

    #[test]
    fn test_parse_symbol() {
        let mut state = ParserState::new("suss");
        let result = parse("foo", &mut state).unwrap();
        assert_eq!(result, Edn::Symbol(Symbol::new("foo")));
    }

    #[test]
    fn test_parse_namespaced_symbol() {
        let mut state = ParserState::new("suss");
        let result = parse("bar/baz", &mut state).unwrap();
        assert_eq!(result, Edn::Symbol(Symbol::namespaced("bar", "baz")));
    }

    #[test]
    fn test_parse_keyword() {
        let mut state = ParserState::new("suss");
        let result = parse(":foo", &mut state).unwrap();
        assert_eq!(result, Edn::Keyword(Keyword::new("foo")));
    }

    #[test]
    fn test_parse_namespaced_keyword() {
        let mut state = ParserState::new("suss");
        let result = parse(":bar/baz", &mut state).unwrap();
        assert_eq!(result, Edn::Keyword(Keyword::namespaced("bar", "baz")));
    }

    #[test]
    fn test_parse_list() {
        let mut state = ParserState::new("suss");
        let result = parse("(1 2 3)", &mut state).unwrap();
        if let Edn::List(items) = result {
            assert_eq!(items.len(), 3);
        } else {
            panic!("Expected list");
        }
    }

    #[test]
    fn test_parse_vector() {
        let mut state = ParserState::new("suss");
        let result = parse("[1 2 3]", &mut state).unwrap();
        if let Edn::Vector(items) = result {
            assert_eq!(items.len(), 3);
        } else {
            panic!("Expected vector");
        }
    }

    #[test]
    fn test_parse_nested() {
        let mut state = ParserState::new("suss");
        let result = parse("(+ 1 (* 2 3))", &mut state).unwrap();
        if let Edn::List(items) = result {
            assert_eq!(items.len(), 3);
            // First item should be symbol +
            assert_eq!(items[0], Edn::Symbol(Symbol::new("+")));
        } else {
            panic!("Expected list");
        }
    }

    #[test]
    fn test_parse_metadata() {
        let mut state = ParserState::new("suss");
        // ^:export should be parsed as a symbol
        let result = parse("(defn ^:export greet [name] name)", &mut state).unwrap();
        if let Edn::List(items) = result {
            assert_eq!(items.len(), 5); // defn, ^:export, greet, [name], name
            // Check that ^:export is parsed as a symbol
            assert_eq!(items[1], Edn::Symbol(Symbol::new("^:export")));
        } else {
            panic!("Expected list");
        }
    }

    #[test]
    fn test_parse_quote() {
        let mut state = ParserState::new("suss");
        let result = parse("'foo", &mut state).unwrap();
        if let Edn::List(items) = result {
            assert_eq!(items.len(), 2);
            assert_eq!(items[0], Edn::Symbol(Symbol::new("quote")));
            assert_eq!(items[1], Edn::Symbol(Symbol::new("foo")));
        } else {
            panic!("Expected list");
        }
    }

    #[test]
    fn test_parse_map() {
        let mut state = ParserState::new("suss");
        let result = parse("{:a 1 :b 2}", &mut state).unwrap();
        if let Edn::Map(pairs) = result {
            assert_eq!(pairs.len(), 2);
        } else {
            panic!("Expected map");
        }
    }

    #[test]
    fn test_parse_set() {
        let mut state = ParserState::new("suss");
        let result = parse("#{1 2 3}", &mut state).unwrap();
        if let Edn::Set(items) = result {
            assert_eq!(items.len(), 3);
        } else {
            panic!("Expected set");
        }
    }
}
