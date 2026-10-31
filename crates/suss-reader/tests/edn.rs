//! Canonical EDN for tooling data (#299 step 1): the printer, the `suss.edn`
//! reading contract and the serde bridge.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use suss_reader::edn::{self, Options, Value};
use suss_reader::{Keyword, Symbol};

fn read(source: &str) -> Value {
    edn::read_one(source, &Options::default()).unwrap_or_else(|error| panic!("{source}: {error}"))
}
fn fails(source: &str) -> edn::Error {
    match edn::read_all(source, &Options::default()) {
        Ok(values) => panic!("{source} read as {values:?}"),
        Err(error) => error,
    }
}
fn printed(source: &str) -> String {
    edn::print(&read(source)).unwrap()
}

#[test]
fn printing_is_canonical() {
    for (source, expected) in [
        ("{:b 2 :a 1 \"z\" 0 nil 3}", "{nil 3, \"z\" 0, :a 1, :b 2}"),
        ("#{:c :a :b}", "#{:a :b :c}"),
        ("#{[2] [1 2] [1]}", "#{[1] [1 2] [2]}"),
        ("[1 1.0 1.5 -0.0 -0 0.1 1e300 1e-7 1e16 123456789012345 ##Inf ##-Inf ##NaN]",
         "[1 1 1.5 -0.0 0 0.1 1e300 1e-7 1e16 123456789012345 ##Inf ##-Inf ##NaN]"),
        ("0x10", "16"),
        ("\"a\\\"b\\\\c\\nd\\te\\rf\\bg\\fh\\u0001\\u007f é ☃\"",
         "\"a\\\"b\\\\c\\nd\\te\\rf\\bg\\fh\\u0001\\u007f é ☃\""),
        ("\\a", "\"a\""),
        ("[:a/b c/d :1 + - -> .. a.b/c]", "[:a/b c/d :1 + - -> .. a.b/c]"),
        ("(1 (2) [3])", "(1 (2) [3])"),
        ("#queue [1 2]", "#queue [1 2]"),
        ("^:k [x]", "^{:k true} [x]"),
        ("^:a ^{:a 2 :b 1} ^T ^\"S\" x", "^{:a true, :b 1, :tag T} x"),
        ("[1 #_ 2 3 #_#_ 4 5]", "[1 3]"),
        ("; comment\n {:a 1,,,} ; more", "{:a 1}"),
    ] {
        assert_eq!(printed(source), expected, "{source}");
    }
}

#[test]
fn pretty_printing_breaks_wide_collections_only() {
    let narrow = read("{:a [1 2] :b #{3}}");
    assert_eq!(edn::print_pretty(&narrow).unwrap(), edn::print(&narrow).unwrap());
    let wide = Value::Map(BTreeMap::from([
        (Value::keyword("cells"), Value::Vector((0..12).map(|i| Value::String(format!("core:cell-{i:04}"))).collect())),
        (Value::keyword("format-version"), Value::Number(2.0)),
        (Value::keyword("tag"), Value::Tagged(Symbol::new("queue"), Box::new(Value::Vector((0..30).map(|i| Value::Number(i.into())).collect())))),
    ]));
    let text = edn::print_pretty(&wide).unwrap();
    assert!(text.starts_with("{:cells [\"core:cell-0000\"\n         \"core:cell-0001\""), "{text}");
    assert!(text.contains(",\n :format-version 2,\n :tag #queue [0\n"), "{text}");
    assert!(text.lines().all(|line| line.len() <= 80), "{text}");
    assert_eq!(edn::read_one(&text, &Options::default()).unwrap(), wide);
}

/// A deterministic generator, so failures reproduce from the seed.
struct Generator(u64);
impl Generator {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn name(&mut self) -> String {
        const FIRST: &[&str] = &["a", "b", "zeta", "x?", "-", "+", "->", "a.b", "*", "<=", "é", "ns-1"];
        const REST: &[&str] = &["", "1", "!", "'", "#", "-x", ":k", ".", "_", "%"];
        format!("{}{}", FIRST[self.below(FIRST.len() as u64) as usize], REST[self.below(REST.len() as u64) as usize])
    }
    fn namespace(&mut self) -> Option<String> {
        (self.below(3) == 0).then(|| self.name())
    }
    fn number(&mut self) -> f64 {
        match self.below(8) {
            0 => [0.0, -0.0, f64::INFINITY, f64::NEG_INFINITY, f64::NAN, f64::MIN_POSITIVE, f64::MAX, 2f64.powi(53)][self.below(8) as usize],
            1 => (self.next() as i64 >> self.below(64)) as f64,
            2 => f64::from_bits(self.next()),
            _ => (self.below(2001) as f64 - 1000.0) / [1.0, 3.0, 10.0, 1e7][self.below(4) as usize],
        }
    }
    fn string(&mut self) -> String {
        (0..self.below(6))
            .map(|_| ['a', '"', '\\', '\n', '\t', '\r', '\u{8}', '\u{c}', '\u{1}', '\u{7f}', 'é', '☃', '😀', ' ', ','][self.below(15) as usize])
            .collect()
    }
    fn meta(&mut self, depth: u32) -> BTreeMap<Value, Value> {
        let mut meta = BTreeMap::new();
        for _ in 0..1 + self.below(2) {
            meta.insert(Value::Keyword(Keyword { namespace: self.namespace(), name: self.name() }), self.value(depth + 1));
        }
        meta
    }
    fn value(&mut self, depth: u32) -> Value {
        let leaf = depth > 4 || self.below(3) == 0;
        let value = match if leaf { self.below(6) } else { 6 + self.below(6) } {
            0 => Value::Nil,
            1 => Value::Bool(self.below(2) == 0),
            2 => Value::Number(self.number()),
            3 => Value::String(self.string()),
            4 => Value::Keyword(Keyword { namespace: self.namespace(), name: self.name() }),
            5 => Value::Symbol(Symbol { namespace: self.namespace(), name: self.name() }),
            6 => Value::List((0..self.below(4)).map(|_| self.value(depth + 1)).collect()),
            7 => Value::Vector((0..self.below(4)).map(|_| self.value(depth + 1)).collect()),
            8 => Value::Map((0..self.below(4)).map(|_| (self.value(depth + 1), self.value(depth + 1))).collect()),
            9 => Value::Set((0..self.below(4)).map(|_| self.value(depth + 1)).collect()),
            10 => Value::Tagged(Symbol::new("queue"), Box::new(Value::Vector((0..self.below(3)).map(|_| self.value(depth + 1)).collect()))),
            _ => Value::Tagged(Symbol::namespaced("suss", "binding"), Box::new(self.value(depth + 1))),
        };
        let carries = matches!(value, Value::Symbol(_) | Value::List(_) | Value::Vector(_) | Value::Map(_) | Value::Set(_) | Value::Tagged(..));
        if carries && self.below(5) == 0 { Value::Meta(Box::new(value), self.meta(depth)) } else { value }
    }
}
/// Structural equality that also compares metadata, which `==` ignores.
fn same(a: &Value, b: &Value) -> bool {
    let entries = |a: &BTreeMap<Value, Value>, b: &BTreeMap<Value, Value>| {
        a.len() == b.len() && a.iter().zip(b).all(|((k, v), (l, w))| same(k, l) && same(v, w))
    };
    match (a, b) {
        (Value::Meta(a, m), Value::Meta(b, n)) => same(a, b) && entries(m, n),
        (Value::Meta(..), _) | (_, Value::Meta(..)) => false,
        (Value::List(a), Value::List(b)) | (Value::Vector(a), Value::Vector(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same(a, b))
        }
        (Value::Set(a), Value::Set(b)) => a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same(a, b)),
        (Value::Map(a), Value::Map(b)) => entries(a, b),
        (Value::Tagged(t, a), Value::Tagged(u, b)) => t == u && same(a, b),
        (a, b) => a == b,
    }
}

#[test]
fn every_printable_value_reads_back_equal_and_reprints_identically() {
    let options = Options::preserving_tags();
    let mut generator = Generator(0x5eed_1234_abcd_0001);
    let (mut printable, mut refused) = (0, 0);
    for _ in 0..5000 {
        let value = generator.value(0);
        let Ok(text) = edn::print(&value) else {
            // Only names that no reader token spells are refused.
            refused += 1;
            continue;
        };
        printable += 1;
        let back = edn::read_one(&text, &options).unwrap_or_else(|error| panic!("{text}: {error}"));
        assert!(same(&back, &value), "{text}\n{back:?}\n{value:?}");
        assert_eq!(edn::print(&back).unwrap(), text);
        let pretty = edn::print_pretty(&value).unwrap();
        assert!(same(&edn::read_one(&pretty, &options).unwrap(), &value), "{pretty}");
    }
    assert!(printable > 2000 && refused > 50, "{printable} printable, {refused} refused");
}

#[test]
fn unreadable_names_are_refused_rather_than_misprinted() {
    for value in [
        Value::Symbol(Symbol::new("nil")),
        Value::Symbol(Symbol::new("1a")),
        Value::Symbol(Symbol::new("a b")),
        Value::Symbol(Symbol::new("'a")),
        Value::Symbol(Symbol::new("")),
        Value::Symbol(Symbol::namespaced("a/b", "c")),
        Value::Keyword(Keyword::new("a b")),
        Value::Keyword(Keyword::new(":a")),
        Value::Tagged(Symbol::new("-x"), Box::new(Value::Nil)),
        Value::Meta(Box::new(Value::Number(1.0)), BTreeMap::from([(Value::Nil, Value::Nil)])),
    ] {
        assert!(edn::print(&value).is_err(), "{value:?}");
    }
}

#[test]
fn printing_is_byte_stable_across_processes() {
    // Hash maps iterate in a per-process random order; canonical EDN must not.
    let data: HashMap<String, Vec<u32>> = (0..200).map(|i| (format!("key-{i}"), vec![i, i * 7])).collect();
    let text = edn::to_string(&data).unwrap();
    if std::env::var_os("SUSS_EDN_STABILITY_CHILD").is_some() {
        println!("<<{text}>>");
        return;
    }
    let mut generator = Generator(42);
    let values: Vec<String> = (0..200).filter_map(|_| edn::print(&generator.value(0)).ok()).collect();
    let mut texts = BTreeSet::new();
    for _ in 0..2 {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["printing_is_byte_stable_across_processes", "--exact", "--nocapture"])
            .env("SUSS_EDN_STABILITY_CHILD", "1")
            .output()
            .unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        let start = stdout.find("<<").unwrap() + 2;
        let end = stdout.find(">>").unwrap();
        texts.insert(stdout[start..end].to_owned());
    }
    texts.insert(text);
    assert_eq!(texts.len(), 1, "{texts:?}");
    // Generated values print the same from an independently seeded run.
    let mut generator = Generator(42);
    let again: Vec<String> = (0..200).filter_map(|_| edn::print(&generator.value(0)).ok()).collect();
    assert_eq!(values, again);
}

#[test]
fn code_only_syntax_and_unknown_tags_fail_with_spans() {
    for (source, message, span) in [
        ("'a", "Quote, syntax-quote, unquote and deref syntax are not EDN", 0..1),
        ("[`a]", "Quote, syntax-quote, unquote and deref syntax are not EDN", 1..2),
        ("~a", "Quote, syntax-quote, unquote and deref syntax are not EDN", 0..1),
        ("{:a @b}", "Quote, syntax-quote, unquote and deref syntax are not EDN", 4..5),
        ("#(inc %)", "Anonymous function syntax is not EDN", 0..2),
        ("#'a", "Var syntax is not EDN", 0..2),
        ("[#?(:clj 1)]", "Reader conditionals are not EDN", 1..3),
        ("#js {}", "No reader function for tag js", 0..6),
        ("#foo 1", "No reader function for tag foo", 0..6),
        ("[#_#foo 1]", "No reader function for tag foo", 3..9),
        ("#queue 1", "Queue literal expects a vector for its elements.", 0..8),
        ("{:a 1 :a 2}", "Duplicate key: :a", 6..8),
        ("#{1 1.0}", "Duplicate key: 1", 4..7),
        ("{:a}", "Map requires an even number of forms", 0..4),
        ("{:a #_ 1}", "Map requires an even number of forms", 0..9),
        ("{:a #queue [] :b}", "Map requires an even number of forms", 0..17),
        ("::a", "Auto-resolved keywords require namespace resolution", 0..3),
        ("1/2", "Unsupported numeric form: ratios and precision suffixes are not portable ordinary numbers", 0..3),
        ("\"\\ud800\"", "EDN strings must be valid Unicode", 0..8),
        ("[1 2", "Unclosed collection; expected ]", 0..4),
        ("^1 [x]", "Metadata must be a keyword, symbol, string or map", 1..2),
    ] {
        let error = fails(source);
        assert_eq!((error.message.as_str(), error.span.clone()), (message, Some(span)), "{source}");
    }
}

#[test]
fn read_one_requires_exactly_one_value() {
    assert_eq!(edn::read_one(" ;x\n", &Options::default()).unwrap_err().message, "Expected an EDN value");
    assert_eq!(edn::read_one("1 2", &Options::default()).unwrap_err().message, "Expected exactly one EDN value");
    assert_eq!(edn::read_all("1 #_2 3", &Options::default()).unwrap(), vec![Value::Number(1.0), Value::Number(3.0)]);
}

#[test]
fn tag_readers_and_defaults_follow_clojure_edn() {
    let mut options = Options::default();
    options.readers.insert(Symbol::namespaced("suss", "binding"), Box::new(|value| match value {
        Value::Number(id) => Ok(Value::String(format!("binding-{id}"))),
        _ => Err("Binding ids are numbers".into()),
    }));
    // A reader takes precedence over the built-in tag.
    options.readers.insert(Symbol::new("queue"), Box::new(|value| Ok(Value::List(vec![value]))));
    assert_eq!(edn::read_one("#suss/binding 12", &options).unwrap(), Value::String("binding-12".into()));
    assert_eq!(edn::read_one("#queue 1", &options).unwrap(), Value::List(vec![Value::Number(1.0)]));
    let error = edn::read_one("[#suss/binding x]", &options).unwrap_err();
    assert_eq!((error.message.as_str(), error.span), ("Binding ids are numbers", Some(1..16)));
    assert_eq!(edn::read_one("#other 1", &options).unwrap_err().message, "No reader function for tag other");
    options.default = Some(Box::new(|tag, value| Ok(Value::Vector(vec![Value::Symbol(tag.clone()), value]))));
    assert_eq!(printed_with("#other 1", &options), "[other 1]");
    assert_eq!(printed_with("#js {}", &Options::preserving_tags()), "#js {}");
}
fn printed_with(source: &str, options: &Options) -> String {
    edn::print(&edn::read_one(source, options).unwrap()).unwrap()
}

#[test]
fn depth_and_size_are_bounded() {
    let deep = format!("{}{}", "[".repeat(2000), "]".repeat(2000));
    let error = fails(&deep);
    assert_eq!(error.message, "EDN nesting exceeds the depth limit");
    let options = Options { max_depth: 5000, ..Options::default() };
    assert!(edn::read_one(&deep, &options).is_ok());
    // Nesting a reader can never exhaust the host stack.
    let deeper = format!("{}{}", "[".repeat(200_000), "]".repeat(200_000));
    assert_eq!(fails(&deeper).message, "EDN nesting exceeds the depth limit");
    let options = Options { work_allowance: 100, ..Options::default() };
    let error = edn::read_one(&format!("[{}]", "1 ".repeat(100)), &options).unwrap_err();
    assert_eq!(error.message, "Reader work budget exhausted");
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct Manifest {
    format_version: u32,
    phase: Option<String>,
    source_path: Option<String>,
    files: BTreeMap<PathBuf, String>,
    pairs: Vec<(String, String)>,
    kind: Kind,
    others: Vec<Kind>,
    ratio: f64,
    offset: i64,
    #[serde(rename = "suss.analyzer/version")]
    version: u8,
}
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Kind {
    Plain,
    Wrapped(String),
    Pair(u8, u8),
    Fields { a: bool },
}

#[test]
fn serde_types_round_trip_through_canonical_edn() {
    let manifest = Manifest {
        format_version: 2,
        phase: Some("macro".into()),
        source_path: None,
        files: BTreeMap::from([(PathBuf::from("/b"), "2".into()), (PathBuf::from("/a"), "1".into())]),
        pairs: vec![("core-import-manifest".into(), "ab".into())],
        kind: Kind::Plain,
        others: vec![Kind::Wrapped("w".into()), Kind::Pair(1, 2), Kind::Fields { a: true }],
        ratio: -0.0,
        offset: -(1 << 53),
        version: 1,
    };
    let text = edn::to_string(&manifest).unwrap();
    assert_eq!(
        text,
        "{:files {\"/a\" \"1\", \"/b\" \"2\"}, :format-version 2, :kind :plain, :offset -9007199254740992, \
         :others [{:wrapped \"w\"} {:pair [1 2]} {:fields {:a true}}], :pairs [[\"core-import-manifest\" \"ab\"]], \
         :phase \"macro\", :ratio -0.0, :source-path nil, :suss.analyzer/version 1}"
    );
    let back: Manifest = edn::from_str(&text).unwrap();
    assert_eq!(back, manifest);
    assert!(back.ratio.is_sign_negative());
    let pretty = edn::to_string_pretty(&manifest).unwrap();
    assert_eq!(edn::from_str::<Manifest>(&pretty).unwrap(), manifest);

    let unknown = text.replace(":phase", ":unexpected 1, :phase");
    assert!(edn::from_str::<Manifest>(&unknown).unwrap_err().message.contains("unknown field"));
    let missing = text.replace(":format-version 2, ", "");
    assert!(edn::from_str::<Manifest>(&missing).unwrap_err().message.contains("missing field"));
    let fraction = text.replace(":format-version 2", ":format-version 2.5");
    assert!(edn::from_str::<Manifest>(&fraction).is_err());
    assert!(edn::from_str::<Manifest>("#suss/x {}").unwrap_err().message.contains("No reader function"));
}

#[test]
fn integers_must_be_exact_in_binary64() {
    assert_eq!(edn::to_string(&(1u64 << 53)).unwrap(), "9007199254740992");
    assert_eq!(edn::to_string(&u64::MAX).unwrap_err().message, "Integer 18446744073709551615 is not exact in binary64");
    assert_eq!(edn::to_string(&(-(1i64 << 53) - 1)).unwrap_err().message, "Integer -9007199254740993 is not exact in binary64");
}

/// The compiler's reader's data, as EDN values, for comparison.
fn form_value(form: &suss_reader::forms::Form) -> Value {
    use suss_reader::forms::Kind;
    let all = |items: &[suss_reader::forms::Form]| items.iter().map(form_value).collect::<Vec<_>>();
    let value = match &form.kind {
        Kind::Nil => Value::Nil,
        Kind::Bool(b) => Value::Bool(*b),
        Kind::Number(x) => Value::Number(*x),
        Kind::String(units) => Value::String(String::from_utf16(units).unwrap()),
        Kind::Symbol(s) => Value::Symbol(s.clone()),
        Kind::Keyword(k) => Value::Keyword(k.clone()),
        Kind::List(x) => Value::List(all(x)),
        Kind::Vector(x) => Value::Vector(all(x)),
        Kind::Set(x) => Value::Set(all(x).into_iter().collect()),
        Kind::Map(x) => Value::Map(all(x).chunks(2).map(|pair| (pair[0].clone(), pair[1].clone())).collect()),
        Kind::Tagged { tag, form } => Value::Tagged(tag.clone(), Box::new(form_value(form))),
        other => panic!("not data: {other:?}"),
    };
    let mut meta = BTreeMap::new();
    for form in form.metadata.iter().rev() {
        match form_value(form) {
            Value::Map(entries) => meta.extend(entries),
            Value::Keyword(k) => {
                meta.insert(Value::Keyword(k), Value::Bool(true));
            }
            tag => {
                meta.insert(Value::keyword("tag"), tag);
            }
        }
    }
    if meta.is_empty() { value } else { Value::Meta(Box::new(value), meta) }
}

/// Both readers lex the same text to equal data, metadata included, and
/// fail the same malformed text with the same message and span.
#[test]
fn the_edn_parser_agrees_with_the_compilers_reader() {
    let corpus = [
        "nil true false", "0 -0 +1 -1.5 1e3 1E-3 -0.0 0x1F -0x10 017 2r1010 36rZZ 9007199254740993 ##Inf ##-Inf ##NaN",
        "\"\" \"plain\" \"\\n\\r\\t\\b\\f\\\"\\\\\" \"\\u00e9\\u2603\" \"\\ud83d\\ude00\" \"\\0\\12\\377\" \"é☃😀\" \"multi\nline\"",
        "\\a \\Z \\é \\newline \\return \\tab \\space \\backspace \\formfeed \\u0041 \\o101 \\( \\; \\\"",
        "a ab/cd a.b/c.d + - -> <=> *x* a' a# a% .. / clojure.core// ns/a:b a:b",
        ":a :a/b :1 :+ :a.b/c",
        "() [] {} #{} (1 [2 {3 #{4}}]) {:a 1, :b [2 3]} #{:a \"b\" 3}",
        "^:k [x] ^{:a 1} {} ^T sym ^\"S\" (x) ^:a ^:b ^{:a 2} [y] #queue [1 2] #queue ^:m []",
        "[1 #_ 2 3] [#_ #_ 4 5 6] (a ; comment\n b) {:a #_ :x 1}",
        "  ,, \u{a0}\u{2003} x\u{2028}y",
    ];
    for source in corpus {
        let forms = suss_reader::forms::read_forms(source)
            .and_then(suss_reader::forms::resolve_conditionals)
            .unwrap_or_else(|error| panic!("{source}: {error:?}"));
        let expected: Vec<Value> = forms.iter().map(form_value).collect();
        let found = edn::read_all(source, &Options::default()).unwrap_or_else(|error| panic!("{source}: {error}"));
        assert_eq!(found.len(), expected.len(), "{source}");
        for (found, expected) in found.iter().zip(&expected) {
            assert!(same(found, expected), "{source}\n{found:?}\n{expected:?}");
        }
    }
    for source in [
        "\"abc", "\"\\x\"", "\"\\u12\"", "\"\\19\"", "\"\\400\"", "\\", "\\ab", "\\ud800", "\\o400", "\\uZZZZ",
        "1/2", "1N", "1.5M", "0x", "08", "1e", "2r2", "40r1", "1a", "##Foo", ":", "::a", "a/", "/a", "a/1", "ns/",
        "(", "[1", "{:a 1", ")", "]", "}", "{:a}", "^1 x", "^:a 1", "#queue 1", "#unknown 1", "#_", "#", "#!", "#1 x",
    ] {
        let expected = suss_reader::forms::read_forms(source).and_then(suss_reader::forms::resolve_conditionals);
        let found = edn::read_all(source, &Options::default());
        match (expected, found) {
            (Err(expected), Err(found)) => {
                assert_eq!((found.message.as_str(), found.span), (expected.message.as_str(), Some(expected.span)), "{source}");
            }
            (expected, found) => panic!("{source}: compiler {expected:?}, edn {found:?}"),
        }
    }
}

#[test]
fn review_edge_cases_round_trip_or_fail_clearly() {
    // Every NaN is the one value `##NaN` prints and reads.
    let negative_nan = Value::Number(-f64::NAN);
    assert_eq!(read(&edn::print(&negative_nan).unwrap()), negative_nan);
    let set = Value::Set(BTreeSet::from([Value::Number(f64::NAN), negative_nan.clone()]));
    assert_eq!(edn::print(&set).unwrap(), "#{##NaN}");
    // Nested metadata compares as its value.
    let nested = Value::Meta(
        Box::new(Value::Meta(Box::new(Value::Vector(vec![])), BTreeMap::from([(Value::keyword("a"), Value::Nil)]))),
        BTreeMap::from([(Value::keyword("b"), Value::Nil)]),
    );
    assert_eq!(nested, Value::Vector(vec![]));
    assert_ne!(nested, Value::Number(1.0));
    // A queue tag prints only a vector, which is all it reads.
    let list_queue = Value::Tagged(Symbol::new("queue"), Box::new(Value::List(vec![])));
    assert_eq!(edn::print(&list_queue).unwrap_err().message, "Queue literal expects a vector for its elements.");
    // Clojure: a tag must be a symbol.
    let error = edn::read_one("#nil 1", &Options::preserving_tags()).unwrap_err();
    assert_eq!((error.message.as_str(), error.span), ("Reader tag must be a symbol", Some(0..4)));
    // Metadata on a metadata map is accepted, and only the map applies.
    assert_eq!(printed("^^:x {:a 1} y"), "^{:a 1} y");
}
