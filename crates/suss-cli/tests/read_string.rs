//! Reading data at runtime (#300): `read-string` and `suss.edn/read-string`.
//! A reader written in Suss, in core, so every target has it without a
//! compiler or host reader. Clojure and `clojure.edn` are the oracle; the
//! compiler's reader is the oracle for lexical rules, messages and positions,
//! through the corpus shared with suss-reader's conformance test.
#[path = "support/compilation_fuel.rs"]
mod compilation_fuel;

use std::io::Write;
use std::process::{Command, Stdio};
use suss_cli::{
    portable_macro_data::FormBridge,
    portable_repl,
    portable_macros::CompiledMacros,
    portable_session::{Session, SessionOptions, SessionValue},
};
use suss_reader::edn::{self, Options, Value};
use suss_reader::forms::Kind;

fn assert_true(session: &mut Session, value: &SessionValue, source: &str) {
    session.collect().unwrap();
    session
        .inspect(value, |store, value| {
            let bits = value
                .unwrap_anyref()
                .and_then(|value| value.as_i31(&store).ok().flatten())
                .map(|value| value.get_u32());
            assert_eq!(bits, Some(4), "{source} is not true");
            Ok(())
        })
        .unwrap();
}

fn truthy(session: &mut Session, macros: &mut CompiledMacros, source: &str) {
    let value = session
        .eval_with_macros(source, macros)
        .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    assert_true(session, &value, source);
}

/// The string a source expression evaluates to.
fn text(session: &mut Session, macros: &mut CompiledMacros, source: &str) -> String {
    let value = session
        .eval_with_macros(source, macros)
        .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    session.collect().unwrap();
    let bridge = FormBridge::new(session).unwrap();
    match &bridge.read(session, &value, 0..1).unwrap().kind {
        Kind::String(units) => String::from_utf16(units).unwrap(),
        kind => panic!("{source}: expected a string: {kind:?}"),
    }
}

/// A Suss string literal for `text`.
fn literal(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A Runtime and a Macro session, each with a macro host, in a namespace
/// that aliases suss.edn and defines a record.
fn sessions() -> Vec<(Session, CompiledMacros)> {
    let options = SessionOptions { source_paths: Vec::new(), fuel_per_operation: 1_000_000_000 };
    [Session::new_repl_with_options(options.clone()), Session::new_macro_with_options(options)]
        .into_iter()
        .map(|session| {
            let mut session = session.unwrap();
            let mut macros = CompiledMacros::new().unwrap();
            macros.set_operation_fuel(compilation_fuel::COMPILATION_FUEL);
            for source in [
                "(ns user.reading (:require [suss.edn :as edn]))",
                "(defrecord R [a b])",
                SAME,
            ] {
                session.eval_with_macros(source, &mut macros).unwrap_or_else(|error| panic!("{source}: {error:?}"));
            }
            (session, macros)
        })
        .collect()
}

/// Structural equality as the Rust conformance test checks it: lists and
/// vectors differ, numbers compare with their sign and NaN equals NaN, and
/// metadata must be equal at every level.
const SAME: &str = "(defn same? [a b]
  (and (= (meta a) (meta b))
       (cond
         (number? a) (and (number? b)
                          (or (and (not= a a) (not= b b))
                              (and (== a b) (== (/ 1 a) (/ 1 b)))))
         (map? a) (and (map? b) (= (count a) (count b))
                       (every? (fn [e] (and (contains? b (key e)) (same? (val e) (get b (key e))))) a))
         (set? a) (and (set? b) (= a b))
         (vector? a) (and (vector? b) (= (count a) (count b)) (every? true? (map same? a b)))
         (seq? a) (and (seq? b) (= (count a) (count b)) (every? true? (map same? a b)))
         :else (= a b))))";

/// The corpus shared with suss-reader's `edn_conformance` test.
fn corpus(key: &str) -> Vec<String> {
    let corpus = edn::read_one(include_str!("../../suss-reader/tests/reader-conformance.edn"), &Options::default()).unwrap();
    let Value::Map(entries) = corpus else { panic!("corpus is a map") };
    let Some(Value::Vector(items)) = entries.get(&Value::keyword(key)) else { panic!("corpus has {key}") };
    items
        .iter()
        .map(|item| match item {
            Value::String(text) => text.clone(),
            other => panic!("corpus entry {other:?}"),
        })
        .collect()
}

#[test]
fn both_readers_agree_with_the_compilers_reader_on_the_shared_corpus() {
    for (mut session, mut macros) in sessions() {
        for source in corpus("valid") {
            // The compiler's reader reads the quoted text; the runtime readers
            // read the same text from a string.
            let quoted = format!("(quote [{source}\n])");
            let string = literal(&format!("[{source}\n]"));
            for reader in ["read-string", "edn/read-string"] {
                truthy(&mut session, &mut macros, &format!("(same? {quoted} ({reader} {string}))"));
            }
        }
    }
}

/// The 0-based UTF-16 offset of a byte offset in `source`.
fn units(source: &str, bytes: usize) -> f64 {
    source[..bytes].encode_utf16().count() as f64
}

/// Each runtime reader's message and offset for every malformed entry.
fn runtime_errors(session: &mut Session, macros: &mut CompiledMacros, reader: &str, sources: &[String]) -> Vec<Value> {
    let attempts = sources
        .iter()
        .map(|source| {
            format!(
                "(try ({reader} {}) :read (catch :default e [(ex-message e) (:offset (ex-data e))]))",
                literal(source)
            )
        })
        .collect::<Vec<_>>()
        .join(" ");
    let printed = text(session, macros, &format!("(pr-str [{attempts}])"));
    match edn::read_one(&printed, &Options::default()).unwrap() {
        Value::Vector(items) => items,
        other => panic!("{other:?}"),
    }
}

#[test]
fn malformed_input_fails_with_the_compilers_message_and_position() {
    // read-string resolves ::k against *ns*, as Clojure does, where the
    // compiler's reader leaves it to the analyzer.
    let code: Vec<String> = corpus("malformed").into_iter().filter(|source| source != "::a").collect();
    let data = corpus("malformed");
    for (mut session, mut macros) in sessions() {
        for (reader, sources) in [("read-string", &code), ("edn/read-string", &data)] {
            let found = runtime_errors(&mut session, &mut macros, reader, sources);
            for (source, found) in sources.iter().zip(found) {
                let expected = if reader == "read-string" {
                    let error = suss_reader::forms::read_forms(source)
                        .and_then(suss_reader::forms::resolve_conditionals)
                        .expect_err(source);
                    (error.message, error.span.start)
                } else {
                    let error = edn::read_all(source, &Options::default()).expect_err(source);
                    (error.message, error.span.unwrap().start)
                };
                let expected = Value::Vector(vec![Value::String(expected.0), Value::Number(units(source, expected.1))]);
                assert_eq!(found, expected, "{reader} {source:?}");
            }
        }
    }
}

#[test]
fn read_string_follows_clojure() {
    for (mut session, mut macros) in sessions() {
        for source in [
            // One form; the rest of the string is not read.
            "(= 1 (read-string \"1 2\"))",
            "(= [1] (read-string \"[1] )\"))",
            "(= :done (read-string {:eof :done} \" ; nothing\\n,\"))",
            "(nil? (read-string {:eof nil} \"#_ 1\"))",
            "(= \"EOF while reading\" (try (read-string \"\") (catch :default e (ex-message e))))",
            // No position metadata, as Clojure's read-string.
            "(nil? (meta (read-string \"(a b)\")))",
            "(nil? (meta (first (read-string \"[(a) [b]]\"))))",
            "(= {:k true} (meta (read-string \"^:k (a)\")))",
            "(= {:tag 'T :a 1} (meta (read-string \"^T ^{:a 1 :tag U} x\")))",
            // Quote and the other prefixes, as Clojure spells them.
            "(= '(quote x) (read-string \"'x\"))",
            "(= '(clojure.core/deref x) (read-string \"@x\"))",
            "(= '(clojure.core/unquote x) (read-string \"~x\"))",
            "(= '(clojure.core/unquote-splicing x) (read-string \"~@x\"))",
            "(= '(var x) (read-string \"#'x\"))",
            "(= '(js-obj \"a\" 1 \"b/c\" 2) (read-string \"#js {:a 1 b/c 2}\"))",
            // Auto-resolved keywords resolve against *ns*.
            "(= :user.reading/k (read-string \"::k\"))",
            "(= :suss.edn/k (read-string \"::edn/k\"))",
            // Only a declared alias resolves, as in Clojure's reader.
            "(= \"Invalid token: ::suss.core/k\" (try (read-string \"::suss.core/k\") (catch :default e (ex-message e))))",
            "(= :user/k (binding [*ns* (the-ns 'user)] (read-string \"::k\")))",
            "(= \"Invalid token: ::nope/k\" (try (read-string \"::nope/k\") (catch :default e (ex-message e))))",
            // Duplicate keys and set elements, as Clojure's reader rejects them.
            "(= \"Duplicate key: :a\" (try (read-string \"{:a 1 :a 2}\") (catch :default e (ex-message e))))",
            "(= \"Duplicate key: 1\" (try (read-string \"#{1 1.0}\") (catch :default e (ex-message e))))",
            // The position is the duplicate's start, also for a collection.
            "(= 6 (:offset (try (read-string \"#{[1] [1]}\") (catch :default e (ex-data e)))))",
            "(= 7 (:offset (try (edn/read-string \"{[1] 1 [1] 2}\") (catch :default e (ex-data e)))))",
            // Characters are one-unit strings; numbers have the spec's diagnostics.
            "(= \"a\" (read-string \"\\\\a\"))",
            "(= \"Unsupported numeric form: ratios and precision suffixes are not portable ordinary numbers\" (try (read-string \"1/2\") (catch :default e (ex-message e))))",
            "(let [x (read-string \"##NaN\")] (not= x x))",
            "(== 9007199254740992 (read-string \"9007199254740993\"))",
            "(== 12157665459056928800 (read-string \"3r2222222222222222222222222222222222222222\"))",
        ] {
            truthy(&mut session, &mut macros, source);
        }
    }
}

#[test]
fn anonymous_functions_and_syntax_quote_expand_as_the_compiler_expands_them() {
    for (mut session, mut macros) in sessions() {
        for source in [
            "(let [form (read-string \"#(+ % %2 %&)\") ps (second form)]
               (and (= 'fn* (first form)) (= 4 (count ps)) (= '& (nth ps 2))
                    (= (list '+ (nth ps 0) (nth ps 1) (nth ps 3)) (nth form 2))
                    (= \"p1__\" (apply str (take 4 (name (nth ps 0)))))
                    (= \"rest__\" (apply str (take 6 (name (nth ps 3)))))))",
            "(= \"Nested anonymous function literals are not allowed\"
                (try (read-string \"#(#(%))\") (catch :default e (ex-message e))))",
            // Syntax quote: the runtime reader's expansion equals the compiler's.
            "(= (read-string \"`(a ~b ~@c [d] {:e f} #{g} #queue [h] \\\"s\\\" 1 :k nil true map edn/read-string)\")
                (quote `(a ~b ~@c [d] {:e f} #{g} #queue [h] \"s\" 1 :k nil true map edn/read-string)))",
            "(= (read-string \"`(if fn* .method String. clojure.core/map suss.core/first nope/x js/Date)\")
                (quote `(if fn* .method String. clojure.core/map suss.core/first nope/x js/Date)))",
            "(= (read-string \"``a\") (quote ``a))",
            // The compiler also quotes the template's source position; read-string
            // has none, as Clojure's has none reading from a string.
            "(= (read-string \"`^:k [a]\")
                '(clojure.core/with-meta (clojure.core/vec (clojure.core/sequence (clojure.core/seq (clojure.core/concat (clojure.core/list (quote user.reading/a))))))
                                         (clojure.core/apply clojure.core/array-map (clojure.core/sequence (clojure.core/seq (clojure.core/concat (clojure.core/list :k) (clojure.core/list true)))))))",
            "(= (read-string \"`()\") (quote `()))",
            // One gensym per name and template.
            "(let [chunks (rest (second (second (read-string \"`(x# x# y#)\"))))
                   names (map (fn [chunk] (second (second chunk))) chunks)]
               (and (= (first names) (second names)) (not= (first names) (nth names 2))
                    (= \"x__\" (apply str (take 3 (name (first names)))))))",
            "(= \"Unquote splicing requires a collection element\"
                (try (read-string \"`~@a\") (catch :default e (ex-message e))))",
        ] {
            truthy(&mut session, &mut macros, source);
        }
    }
}

#[test]
fn reader_conditionals_need_opts_as_in_clojure() {
    for (mut session, mut macros) in sessions() {
        for source in [
            "(= \"Conditional read not allowed\" (try (read-string \"#?(:clj 1)\") (catch :default e (ex-message e))))",
            "(= 2 (read-string {:read-cond :allow} \"#?(:clj 1 :suss 2)\"))",
            "(= 2 (read-string {:read-cond :allow} \"#?(:clj 1 :cljs 2)\"))",
            "(= 3 (read-string {:read-cond :allow} \"#?(:clj 1 :default 3)\"))",
            "(= 1 (read-string {:read-cond :allow :features #{:clj}} \"#?(:clj 1 :suss 2)\"))",
            "(= [1 2 3] (read-string {:read-cond :allow} \"[#?@(:cljs [1 2]) 3]\"))",
            "(= :none (read-string {:read-cond :allow :eof :none} \"#?(:clj 1)\"))",
            // Unselected branches are read without resolving their tags.
            "(= [] (read-string {:read-cond :allow} \"[#?(:clj #foo 1)]\"))",
            "(= \"Splicing conditional requires a delimited collection\"
                (try (read-string {:read-cond :allow} \"#?@(:cljs [1])\") (catch :default e (ex-message e))))",
            "(= \"Reader feature must be a keyword\"
                (try (read-string {:read-cond :allow} \"#?(clj 1)\") (catch :default e (ex-message e))))",
            "(= \"Reader conditional requires feature/form pairs\"
                (try (read-string {:read-cond :allow} \"#?(:clj)\") (catch :default e (ex-message e))))",
            "(= \"Reader conditional requires feature/form pairs\"
                (try (read-string {:read-cond :allow} \"#?(:default #_1)\") (catch :default e (ex-message e))))",
            "(= (reader-conditional '(:clj [1]) true) (read-string {:read-cond :preserve} \"#?@(:clj [1])\"))",
            // :preserve keeps conditionals and tags as data.
            "(= (reader-conditional '(:clj 1 :cljs 2) false) (read-string {:read-cond :preserve} \"#?(:clj 1 :cljs 2)\"))",
            "(let [c (first (read-string {:read-cond :preserve} \"[#?@(:clj [1])]\"))]
               (and (reader-conditional? c) (:splicing? c) (= '(:clj [1]) (:form c)) (= \"#?@(:clj [1])\" (pr-str c))))",
            "(= (tagged-literal 'foo 1) (read-string {:read-cond :preserve} \"#foo 1\"))",
            "(= \"#foo [1]\" (pr-str (tagged-literal 'foo [1])))",
            "(tagged-literal? (read-string {:read-cond :preserve} \"#user.reading.R[1 2]\"))",
        ] {
            truthy(&mut session, &mut macros, source);
        }
    }
}

#[test]
fn tags_and_record_literals_follow_clojure() {
    for (mut session, mut macros) in sessions() {
        for source in [
            "(= [:x 1] (binding [*data-readers* {'x (fn [v] [:x v])}] (read-string \"#x 1\")))",
            "(= ['y 1] (binding [*default-data-reader-fn* (fn [t v] [t v])] (read-string \"#y 1\")))",
            "(= \"No reader function for tag y\" (try (read-string \"#y 1\") (catch :default e (ex-message e))))",
            "(= (into (.-EMPTY PersistentQueue) [1 2]) (read-string \"#queue [1 2]\"))",
            // Record literals construct records while *read-eval* is true.
            "(= (->R 1 2) (read-string \"#user.reading.R{:a 1 :b 2}\"))",
            "(= (->R 1 2) (read-string \"#user.reading.R[1 2]\"))",
            "(= (map->R {:a 1 :c 3}) (read-string \"#user.reading.R{:a 1 :c 3}\"))",
            "(= (->R 1 [2]) (read-string (pr-str (->R 1 [2]))))",
            "(= \"Record construction syntax can only be used when *read-eval* == true\"
                (binding [*read-eval* false] (try (read-string \"#user.reading.R[1 2]\") (catch :default e (ex-message e)))))",
            "(= \"Unexpected number of constructor arguments to user.reading.R: got 3\"
                (try (read-string \"#user.reading.R[1 2 3]\") (catch :default e (ex-message e))))",
            "(= \"Unreadable defrecord form: key must be a keyword, got \\\"a\\\"\"
                (try (read-string \"#user.reading.R{\\\"a\\\" 1}\") (catch :default e (ex-message e))))",
            "(= \"No record type for tag user.reading.Nope\" (try (read-string \"#user.reading.Nope{}\") (catch :default e (ex-message e))))",
            // Non-data values do not read.
            "(= \"No reader function for tag object\" (try (read-string \"#object[Foo 0x1 \\\"x\\\"]\") (catch :default e (ex-message e))))",
        ] {
            truthy(&mut session, &mut macros, source);
        }
    }
}

#[test]
fn suss_edn_follows_clojure_edn() {
    for (mut session, mut macros) in sessions() {
        for source in [
            "(nil? (edn/read-string nil))",
            "(nil? (edn/read-string \"\"))",
            "(= \"EOF while reading\" (try (edn/read-string {} \"\") (catch :default e (ex-message e))))",
            "(= {:a [1 2]} (edn/read-string \"{:a [1 2]} trailing\"))",
            "(= [:x 1] (edn/read-string {:readers {'x (fn [v] [:x v])}} \"#x 1\"))",
            "(= '[y 1] (edn/read-string {:default (fn [t v] [t v])} \"#y 1\"))",
            "(= '(1) (edn/read-string {:readers {'queue list}} \"#queue 1\"))",
            "(= \"No reader function for tag y\" (try (edn/read-string \"#y 1\") (catch :default e (ex-message e))))",
            "(= \"No reader function for tag user.reading.R\" (try (edn/read-string \"#user.reading.R[1 2]\") (catch :default e (ex-message e))))",
            "(= [1 2] (edn/read-string {:readers {'user.reading.R vec}} \"#user.reading.R[1 2]\"))",
            "(= \"No reader function for tag object\" (try (edn/read-string \"#object[Foo 0x1 \\\"x\\\"]\") (catch :default e (ex-message e))))",
            "(= \"Quote, syntax-quote, unquote and deref syntax are not EDN\" (try (edn/read-string \"'a\") (catch :default e (ex-message e))))",
            "(= \"Reader conditionals are not EDN\" (try (edn/read-string \"#?(:clj 1)\") (catch :default e (ex-message e))))",
            "(= \"Auto-resolved keywords require namespace resolution\" (try (edn/read-string \"::a\") (catch :default e (ex-message e))))",
            "(= \"No reader function for tag js\" (try (edn/read-string \"#js {}\") (catch :default e (ex-message e))))",
            "(= \"Reader tag must be a symbol\" (try (edn/read-string \"#nil 1\") (catch :default e (ex-message e))))",
            // Suss strings may hold lone surrogates, as Clojure's may.
            "(= 1 (count (edn/read-string \"\\\"\\\\ud800\\\"\")))",
        ] {
            truthy(&mut session, &mut macros, source);
        }
    }
}

#[test]
fn errors_are_catchable_and_carry_their_position() {
    for (mut session, mut macros) in sessions() {
        for source in [
            "(= {:type :reader-exception :ex-kind :reader-error :offset 7 :line 2 :col 5}
                (try (read-string \"[1\\n  2 )\") (catch :default e (ex-data e))))",
            "(= {:type :reader-exception :ex-kind :eof :offset 4 :line 2 :col 2}
                (try (read-string \"[1\\n [\") (catch :default e (ex-data e))))",
            "(= {:type :reader-exception :ex-kind :eof :offset 1 :line 1 :col 2}
                (try (edn/read-string \"[\\\"abc\") (catch :default e (ex-data e))))",
            "(= \"Unclosed string\" (try (edn/read-string \"[\\\"abc\") (catch :default e (ex-message e))))",
        ] {
            truthy(&mut session, &mut macros, source);
        }
    }
}

#[test]
fn nesting_depth_is_bounded_and_catchable() {
    for (mut session, mut macros) in sessions() {
        let deep = |depth: usize| literal(&format!("{}{}", "[".repeat(depth), "]".repeat(depth)));
        for (reader, message) in [
            ("read-string", "Reader nesting exceeds the depth limit"),
            ("edn/read-string", "EDN nesting exceeds the depth limit"),
        ] {
            truthy(&mut session, &mut macros, &format!("(vector? ({reader} {}))", deep(128)));
            for depth in [129, 100_000] {
                truthy(
                    &mut session,
                    &mut macros,
                    &format!("(= {} (try ({reader} {}) (catch :default e (ex-message e))))", literal(message), deep(depth)),
                );
            }
        }
        // Each nested syntax quote expands about four times deeper; the
        // expansion counts against the bound.
        truthy(&mut session, &mut macros, "(seq? (read-string \"```x\"))");
        truthy(
            &mut session,
            &mut macros,
            "(= \"Reader nesting exceeds the depth limit\" (try (read-string \"```````x\") (catch :default e (ex-message e))))",
        );
        // Prefixes nest as deeply as collections do.
        truthy(
            &mut session,
            &mut macros,
            &format!(
                "(= \"Reader nesting exceeds the depth limit\" (try (read-string {}) (catch :default e (ex-message e))))",
                literal(&format!("{}x", "'".repeat(10_000)))
            ),
        );
    }
}

#[test]
fn reading_is_charged_to_the_callers_fuel() {
    let mut session = Session::new_repl_with_options(SessionOptions {
        source_paths: Vec::new(),
        fuel_per_operation: 1_000_000_000,
    })
    .unwrap();
    let item = "[1 :a \\\"s\\\"] ";
    session.eval(&format!("(def small \"[{}]\")", item.repeat(100))).unwrap();
    session.eval(&format!("(def large \"[{}]\")", item.repeat(20_000))).unwrap();
    // Within the budget reading succeeds; a long input exhausts it and the
    // session goes on.
    session.set_operation_fuel(20_000_000);
    session.eval("(count (read-string small))").unwrap();
    let error = session.eval("(count (read-string large))").unwrap_err();
    assert!(format!("{error:?}").contains("fuel"), "{error:?}");
    session.eval("(count (read-string small))").unwrap();
}

#[test]
fn printed_data_reads_back_equal() {
    for (mut session, mut macros) in sessions() {
        for source in [
            "(let [x [nil true false 0 -1.5 1e300 ##Inf ##-Inf \"a\\\"\\\\\\n\\t\\u0001é☃😀\" \\a \\newline :k :a/b 'sym 'a/b
                      '() '(1 (2)) [] [1 [2]] {} {:a {:b 1}} #{} #{1 #{2}}
                      (zipmap (range 20) (map str (range 20))) (set (range 40))
                      #queue [1 [2]] (range 3) (map inc [1 2]) (seq [1 2]) (first {:a 1}) (subvec [1 2 3] 1)
                      (->R 1 [2 {:c #{3}}]) (map->R {:a 1 :b 2 :z 3})]]
               (and (= x (read-string (pr-str x))) (same? (read-string (pr-str x)) (read-string (pr-str x)))))",
            // Sorted collections read back as plain maps and sets, equal to them.
            "(let [m (sorted-map :b 2 :a 1) s (sorted-set 3 1 2) rm (read-string (pr-str m)) rs (read-string (pr-str s))]
               (and (= m rm) (= s rs) (map? rm) (set? rs) (not (instance? PersistentTreeMap rm)) (not (instance? PersistentTreeSet rs))))",
            // Metadata, where printed.
            "(binding [*print-meta* true]
               (let [x (with-meta [1 (with-meta 'a {:b 2}) (with-meta {:c 3} {:d 4})] {:k true})]
                 (same? x (read-string (pr-str x)))))",
            // Data without records reads back through suss.edn as well.
            "(let [x [nil 1.5 \"s\" :k 'a/b '(1) {:a #{2}} #queue [3] (sorted-map 1 2)]]
               (= x (edn/read-string (pr-str x))))",
        ] {
            truthy(&mut session, &mut macros, source);
        }
        // Values that are not data do not read back as equal data, as in
        // Clojure: functions, atoms and namespaces print as #object forms
        // (or, until #304, do not print at all), and a type prints as its name.
        for source in ["(fn [] 1)", "(atom 1)", "*ns*"] {
            truthy(
                &mut session,
                &mut macros,
                &format!("(try (not= {source} (read-string (pr-str {source}))) (catch :default e true))"),
            );
        }
    }
}

#[test]
fn macros_can_read_at_expansion_time() {
    // Expansion runs in the Macro phase; the REPL defines the macros.
    for (mut session, mut macros) in sessions().into_iter().take(1) {
        for (source, expected) in [
            // suss.edn is loaded in the Macro phase as any namespace a macro uses.
            ("(ns user.reading (:require [suss.edn :as edn]) (:require-macros [suss.edn]))", None),
            ("(defmacro read-at-expansion [text] (read-string text))", None),
            ("(defmacro read-edn-at-expansion [text] (list 'quote (suss.edn/read-string text)))", None),
            ("(= 3 (read-at-expansion \"(+ 1 2)\"))", Some("true")),
            ("(= :user.reading/k (read-at-expansion \"::k\"))", Some("true")),
            ("(= {:a [1]} (read-edn-at-expansion \"{:a [1]}\"))", Some("true")),
        ] {
            let printed = portable_repl::evaluate_compiled(&mut session, &mut macros, source)
                .unwrap_or_else(|error| panic!("{source}: {error:?}"));
            if let Some(expected) = expected {
                assert_eq!(printed, expected, "{source}");
            }
        }
    }
}

#[test]
fn the_repl_and_script_files_read() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_suss"))
        .arg("repl")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"(read-string \"[1 :a ::b]\")\n(ns other (:require [suss.edn :as edn]))\n(edn/read-string \"{:a #{1}}\")\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    let out = String::from_utf8(output.stdout).unwrap();
    let errors = String::from_utf8(output.stderr).unwrap();
    let lines = out.lines().filter(|line| *line != "...").collect::<Vec<_>>();
    assert_eq!(lines, ["[1 :a :user/b]", "nil", "{:a #{1}}"], "{out}\n{errors}");

    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("app.sus");
    std::fs::write(
        &file,
        concat!(
            "(ns app (:require [suss.edn :as edn]))\n",
            "(when-not (= \"[:app/k #queue [1]]\" (pr-str (read-string \"[::k #queue [1]]\"))) (throw (ex-info \"code\" {})))\n",
            "(when-not (= '(1 \"two\") (edn/read-string \"(1 \\\"two\\\")\")) (throw (ex-info \"data\" {})))\n",
        ),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_suss")).arg(&file).output().unwrap();
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn components_read_without_a_compiler_or_host_reader() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("src/app")).unwrap();
    std::fs::write(
        root.path().join("src/app/reading.sus"),
        concat!(
            "(ns app.reading (:require [suss.edn :as edn]))\n",
            "(defrecord P [x y])\n",
            "(defn code [] (pr-str (read-string {:read-cond :allow} \"[::k #?(:suss 2 :clj 3) #app.reading.P[1 2]]\")))\n",
            "(defn data [] (pr-str (edn/read-string \"{:a [1 2] :b #queue [3]}\")))\n",
        ),
    )
    .unwrap();
    std::fs::write(
        root.path().join("reading.wit"),
        "package test:reading;\nworld reading {\n export code: func() -> string;\n export data: func() -> string;\n}\n",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(root.path())
        .args(["compile", "-n", "app.reading", "--src", "src", "-w", "reading.wit", "--wit-world", "reading", "-o", "reading.wasm"])
        .arg("--compile-fuel")
        .arg(compilation_fuel::COMPILATION_FUEL.to_string())
        .args(["--export", "code=app.reading/code", "--export", "data=app.reading/data"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    // The component links no reader interface: reading is Suss code in core.
    let wasm = std::fs::read(root.path().join("reading.wasm")).unwrap();
    assert!(!wasm.windows(b"suss:lang/reader".len()).any(|window| window == b"suss:lang/reader"));
    for (export, expected) in [
        // An export called by the host runs with *ns* at its root, suss.core,
        // so ::k resolves there, as in Clojure outside any load.
        ("code", "[:suss.core/k 2 #app.reading.P{:x 1, :y 2}]"),
        ("data", "{:a [1 2], :b #queue [3]}"),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_suss"))
            .current_dir(root.path())
            .args(["run", "reading.wasm", "--invoke", export])
            .output()
            .unwrap();
        assert!(output.status.success(), "{export}: {}", String::from_utf8_lossy(&output.stderr));
        let out = String::from_utf8(output.stdout).unwrap();
        let printed: String = unquoted(out.trim());
        assert_eq!(printed, expected, "{export}");
    }
}

/// `suss run` prints a returned string as a quoted literal.
fn unquoted(text: &str) -> String {
    match edn::read_one(text, &Options::default()) {
        Ok(Value::String(text)) => text,
        _ => text.to_owned(),
    }
}
