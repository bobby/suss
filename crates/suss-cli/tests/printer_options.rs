//! The printer's dynamic options (#302): `*print-length*`, `*print-level*`,
//! `*print-meta*`, `*print-readably*`, `*print-namespace-maps*`,
//! `*print-dup*`, `*verbose-defrecords*` and `*flush-on-newline*`, through
//! every string printer and `str`, in both compiled phases and in components,
//! and the `print-method` and `print-dup` extension points.
//! Expected text is Clojure's (`clojure/core_print.clj`).
#[path = "support/compilation_fuel.rs"]
mod compilation_fuel;

use std::process::Command;
use suss_cli::{
    portable_macro_data::FormBridge,
    portable_macros::CompiledMacros,
    portable_session::{Session, SessionOptions},
};
use suss_reader::edn::{self, Options, Value};
use suss_reader::forms::Kind;

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

/// A Runtime and a Macro session, each with a macro host, in a namespace
/// that defines a record.
fn sessions() -> Vec<(Session, CompiledMacros)> {
    let options = SessionOptions { source_paths: Vec::new(), fuel_per_operation: 1_000_000_000 };
    [Session::new_repl_with_options(options.clone()), Session::new_macro_with_options(options)]
        .into_iter()
        .map(|session| {
            let mut session = session.unwrap();
            let mut macros = CompiledMacros::new().unwrap();
            macros.set_operation_fuel(compilation_fuel::COMPILATION_FUEL);
            for source in ["(ns user.printing)", "(defrecord R [a b])", SAME] {
                session.eval_with_macros(source, &mut macros).unwrap_or_else(|error| panic!("{source}: {error:?}"));
            }
            (session, macros)
        })
        .collect()
}

/// Each expression evaluates to the expected string in both phases.
fn check(cases: &[(&str, &str)]) {
    for (mut session, mut macros) in sessions() {
        for (source, expected) in cases {
            assert_eq!(text(&mut session, &mut macros, source), *expected, "{source}");
        }
    }
}

/// Structural equality with metadata at every level, as read_string.rs checks it.
const SAME: &str = "(defn same? [a b]
  (and (= (meta a) (meta b))
       (cond
         (map? a) (and (map? b) (= (count a) (count b))
                       (every? (fn [e] (and (contains? b (key e)) (same? (val e) (get b (key e))))) a))
         (vector? a) (and (vector? b) (= (count a) (count b)) (every? true? (map same? a b)))
         (seq? a) (and (seq? b) (= (count a) (count b)) (every? true? (map same? a b)))
         :else (= a b))))";

#[test]
fn print_length_limits_every_collection() {
    check(&[
        ("(binding [*print-length* 2] (pr-str [1 2 3 4]))", "[1 2 ...]"),
        ("(binding [*print-length* 2] (pr-str [1 2]))", "[1 2]"),
        ("(binding [*print-length* 0] (pr-str [1 2]))", "[...]"),
        ("(binding [*print-length* 0] (pr-str []))", "[]"),
        ("(binding [*print-length* 3] (pr-str (range)))", "(0 1 2 ...)"),
        ("(binding [*print-length* 2] (pr-str '(1 2 3)))", "(1 2 ...)"),
        ("(binding [*print-length* 2] (pr-str {:a 1 :b 2 :c 3}))", "{:a 1, :b 2, ...}"),
        ("(binding [*print-length* 0] (pr-str #{1}))", "#{...}"),
        ("(binding [*print-length* 2] (pr-str [[1 2 3] [4 5 6]]))", "[[1 2 ...] [4 5 ...]]"),
        ("(binding [*print-length* 2] (pr-str (repeat [1 2 3])))", "([1 2 ...] [1 2 ...] ...)"),
        // Logical false is no limit.
        ("(binding [*print-length* false] (pr-str [1 2 3]))", "[1 2 3]"),
        // A record's entries are counted as a map's are.
        ("(binding [*print-length* 1] (pr-str (->R 1 2)))", "#user.printing.R{:a 1, ...}"),
        ("(binding [*print-length* 0] (pr-str (->R 1 2)))", "#user.printing.R{...}"),
    ]);
}

#[test]
fn print_length_bounds_how_much_of_a_lazy_seq_is_realized() {
    // Clojure's print-sequential destructures [x & xs] on every step, so it
    // realizes two elements past the limit of an unchunked seq, and none
    // when the level hides the seq.
    let effects = |binding: &str| {
        format!(
            "(let [n (atom 0) s (map (fn [x] (swap! n inc) x) (iterate inc 0))] \
               (pr-str [(binding [{binding}] (pr-str s)) @n]))"
        )
    };
    check(&[
        (&effects("*print-length* 2"), "[\"(0 1 ...)\" 4]"),
        (&effects("*print-length* 0"), "[\"(...)\" 2]"),
        (&effects("*print-length* 2 *print-level* 0"), "[\"#\" 0]"),
        ("(let [n (atom 0) s (map (fn [x] (swap! n inc) x) (iterate inc 0))] \
            (pr-str [(binding [*print-length* 1] (pr-str [s s])) @n]))", "[\"[(0 ...) ...]\" 3]"),
    ]);
}

#[test]
fn print_level_hides_deep_collections() {
    check(&[
        ("(binding [*print-level* 1] (pr-str [1 [2 [3]]]))", "[1 #]"),
        ("(binding [*print-level* 2] (pr-str [1 [2 [3]]]))", "[1 [2 #]]"),
        ("(binding [*print-level* 0] (pr-str [1]))", "#"),
        // Only collections count; atoms print at any level.
        ("(binding [*print-level* 0] (pr-str 1))", "1"),
        ("(binding [*print-level* 0] (pr-str \"s\"))", "\"s\""),
        ("(binding [*print-level* 1] (pr-str '(1 (2))))", "(1 #)"),
        // Map entries are not levels of their own.
        ("(binding [*print-level* 2] (pr-str {:a {:b {:c 1}}}))", "{:a {:b #}}"),
        ("(binding [*print-level* 1] (pr-str {:a [1]}))", "{:a #}"),
        ("(binding [*print-level* 1] (pr-str (->R [1] 2)))", "#user.printing.R{:a #, :b 2}"),
        // Clojure writes a record's name before the level check.
        ("(binding [*print-level* 0] (pr-str (->R 1 2)))", "#user.printing.R#"),
        ("(binding [*print-level* 1] (pr-str [(->R 1 2)]))", "[#user.printing.R#]"),
        ("(binding [*print-level* false] (pr-str [[1]]))", "[[1]]"),
        // Metadata prints at the level of the value it belongs to.
        ("(binding [*print-level* 1 *print-meta* true] (pr-str (with-meta [[1]] {:a [1]})))", "^{:a #} [#]"),
    ]);
}

#[test]
fn print_meta_prints_metadata_readably() {
    check(&[
        ("(binding [*print-meta* true] (pr-str (with-meta [1] {:k 1})))", "^{:k 1} [1]"),
        ("(binding [*print-meta* true] (pr-str [(with-meta {} {:a 1})]))", "[^{:a 1} {}]"),
        ("(binding [*print-meta* true] (pr-str (with-meta '(1) {:a 1})))", "^{:a 1} (1)"),
        ("(binding [*print-meta* true] (pr-str (with-meta #{} {:a 1})))", "^{:a 1} #{}"),
        // A lone :tag prints as a type hint.
        ("(binding [*print-meta* true] (pr-str (with-meta 'x {:tag 'long})))", "^long x"),
        ("(binding [*print-meta* true] (pr-str (with-meta 'x {:tag 'long :a 1})))", "^{:tag long, :a 1} x"),
        // Empty metadata, and metadata off, print nothing.
        ("(binding [*print-meta* true] (pr-str (with-meta [1] {})))", "[1]"),
        ("(pr-str (with-meta [1] {:k 1}))", "[1]"),
        // A keyword :type is the print-method dispatch value, not printed.
        ("(binding [*print-meta* true] (pr-str (with-meta [1] {:type :t :a 1})))", "^{:a 1} [1]"),
        ("(binding [*print-meta* true] (pr-str (with-meta [1] {:type :t})))", "[1]"),
        ("(binding [*print-meta* true] (pr-str (with-meta [1] {:type \"t\"})))", "^{:type \"t\"} [1]"),
        // Records.
        ("(binding [*print-meta* true] (pr-str (with-meta (->R 1 2) {:m 1})))", "^{:m 1} #user.printing.R{:a 1, :b 2}"),
        // Only readable printing prints metadata.
        ("(binding [*print-meta* true] (print-str (with-meta [1] {:k 1})))", "[1]"),
        ("(binding [*print-meta* true] (println-str (with-meta [1] {:k 1})))", "[1]\n"),
        ("(binding [*print-meta* true] (prn-str (with-meta [1] {:k 1})))", "^{:k 1} [1]\n"),
        ("(binding [*print-meta* true *print-readably* false] (pr-str (with-meta [1] {:k 1})))", "[1]"),
    ]);
}

#[test]
fn print_meta_output_reads_back_with_equal_metadata() {
    for (mut session, mut macros) in sessions() {
        for value in [
            "(with-meta [(with-meta 'a {:tag 'long}) (with-meta {:b (with-meta '(1) {:l true})} {:m 1})] {:top \"x\"})",
            "(with-meta (->R (with-meta [1] {:f 1}) 2) {:m 1})",
            "(with-meta {:k (with-meta 'v {:tag 'String :line 3})} {:a {:nested [1]}})",
        ] {
            let source = format!(
                "(let [x {value} y (read-string (binding [*print-meta* true] (pr-str x)))] (pr-str (same? x y)))"
            );
            assert_eq!(text(&mut session, &mut macros, &source), "true", "{value}");
        }
    }
}

#[test]
fn every_printer_and_str_observe_the_options() {
    check(&[
        ("(binding [*print-length* 1] (prn-str [1 2] '(3 4)))", "[1 ...] (3 ...)\n"),
        ("(binding [*print-length* 1] (print-str [\"a\" \"b\"]))", "[a ...]"),
        ("(binding [*print-level* 1] (println-str [[1]] \"x\"))", "[#] x\n"),
        // str of a collection prints it as pr does, as Clojure's toString.
        ("(binding [*print-length* 1] (str [1 2] '(3 4) {:a 1 :b 2} #{5}))", "[1 ...](3 ...){:a 1, ...}#{5}"),
        ("(binding [*print-level* 1] (str [[1]]))", "[#]"),
        ("(binding [*print-meta* true] (str (with-meta [1] {:k 1})))", "^{:k 1} [1]"),
        ("(binding [*print-meta* true] (str [(with-meta 'x {:k 1})]))", "[^{:k 1} x]"),
        ("(binding [*print-readably* false] (str [\"a\"]))", "[a]"),
        ("(binding [*print-readably* false] (prn-str [\"a\"]))", "[a]\n"),
        ("(binding [*print-namespace-maps* true] (str {:a/b 1}))", "#:a{:b 1}"),
        ("(binding [*print-namespace-maps* true] (println-str {:a/b 1}))", "#:a{:b 1}\n"),
        ("(binding [*print-dup* true *print-length* 0] (prn-str (->R 1 [2])))", "#user.printing.R[1, [2]]\n"),
        ("(binding [*print-dup* true *print-length* 0] (str [[1]]))", "[[1]]"),
        ("(str [\"a\"])", "[\"a\"]"),
    ]);
}

#[test]
fn user_printers_observe_the_options() {
    for (mut session, mut macros) in sessions() {
        for source in [
            "(deftype Pair [a b] IPrintWithWriter (-pr-writer [_ w opts] (pr-sequential-writer w pr-writer \"#pair[\" \" \" \"]\" opts [a b])))",
            "(defrecord Box [v])",
            "(extend-type Box IPrintWithWriter (-pr-writer [this w opts] (-write w \"#box \") (pr-writer (:v this) w opts)))",
        ] {
            session.eval_with_macros(source, &mut macros).unwrap_or_else(|error| panic!("{source}: {error:?}"));
        }
        for (source, expected) in [
            ("(pr-str (Pair. 1 [2 3]))", "#pair[1 [2 3]]"),
            ("(binding [*print-length* 1] (pr-str (Pair. 1 [2 3])))", "#pair[1 ...]"),
            ("(binding [*print-level* 1] (pr-str (Pair. 1 [2 3])))", "#pair[1 #]"),
            ("(binding [*print-level* 1] (pr-str [(Pair. 1 2)]))", "[#]"),
            ("(binding [*print-length* 1] (pr-str (->Box [1 2])))", "#box [1 ...]"),
            ("(binding [*print-meta* true] (pr-str (with-meta (->Box 1) {:m 1})))", "^{:m 1} #box 1"),
        ] {
            assert_eq!(text(&mut session, &mut macros, source), expected, "{source}");
        }
    }
}

/// Evaluates each definition, then checks each expression's string, in both phases.
fn check_after(definitions: &[&str], cases: &[(&str, &str)]) {
    for (mut session, mut macros) in sessions() {
        for source in definitions {
            session.eval_with_macros(source, &mut macros).unwrap_or_else(|error| panic!("{source}: {error:?}"));
        }
        for (source, expected) in cases {
            assert_eq!(text(&mut session, &mut macros, source), *expected, "{source}");
        }
    }
}

// Each expected string below is Clojure 1.12.6's for the same definitions.
const PRINT_METHODS: &[&str] = &[
    "(deftype T [a])",
    "(deftype S [a])",
    "(do (defmethod print-method T [t w] (.write w \"#t \") (print-method (.-a t) w)) nil)",
    "(do (defmethod print-method R [r w] (.write w \"#r\") (print-method (into {} r) w)) nil)",
    "(do (defmethod print-method ::k [x w] (.write w \"K:\") (print-method (vec x) w)) nil)",
    "(do (defmethod print-method S [s w] (print-simple (.-a s) w)) nil)",
];

#[test]
fn print_method_prints_user_types_and_records() {
    check_after(PRINT_METHODS, &[
        ("(pr-str [1 (T. [2 3]) \"x\"])", "[1 #t [2 3] \"x\"]"),
        ("(pr-str (T. [1 (T. 2)]))", "#t [1 #t 2]"),
        ("(pr-str (->R 1 [2]))", "#r{:a 1, :b [2]}"),
        // Every printer, and str of a collection, prints through print-method.
        ("(print-str (T. \"s\"))", "#t s"),
        ("(prn-str (T. 1))", "#t 1\n"),
        ("(str [(T. 5)])", "[#t 5]"),
        // A method replaces the type's built-in printer, so it prints no metadata.
        ("(binding [*print-meta* true] (pr-str (with-meta (->R 1 2) {:m 1})))", "#r{:a 1, :b 2}"),
        ("(binding [*print-meta* true] (pr-str (T. (with-meta [1] {:m 1}))))", "#t ^{:m 1} [1]"),
        // print-simple prints metadata and then str, which prints it again.
        ("(pr-str (S. [1]))", "[1]"),
        ("(binding [*print-meta* true] (pr-str (S. (with-meta [1] {:tag 'x}))))", "^x ^x [1]"),
        ("(binding [*print-meta* true] (pr-str (S. (with-meta [1] {:a 1 :type ::z}))))", "^{:a 1, :type :user.printing/z} ^{:a 1} [1]"),
    ]);
}

#[test]
fn print_method_observes_length_and_level() {
    check_after(PRINT_METHODS, &[
        ("(binding [*print-length* 1] (pr-str (T. [2 3])))", "#t [2 ...]"),
        ("(binding [*print-level* 1] (pr-str [(T. [2 3])]))", "[#t #]"),
        ("(binding [*print-level* 0] (pr-str (T. [2 3])))", "#t #"),
        ("(binding [*print-length* 1] (pr-str (->R 1 [2 3])))", "#r{:a 1, ...}"),
        ("(binding [*print-level* 1] (pr-str [(->R 1 [2])]))", "[#r#]"),
    ]);
}

#[test]
fn print_method_dispatches_on_a_keyword_type() {
    check_after(PRINT_METHODS, &[
        ("(pr-str (with-meta [1 2] {:type ::k}))", "K:[1 2]"),
        ("(binding [*print-meta* true] (pr-str (with-meta [1 2] {:type ::k :a 1})))", "K:[1 2]"),
        // A :type with no method prints without it, through the type's method.
        ("(pr-str (with-meta [1] {:type ::other :a 1}))", "[1]"),
        ("(binding [*print-meta* true] (pr-str (with-meta [1] {:type ::other :a 1})))", "^{:a 1} [1]"),
        ("(pr-str (with-meta (->R 1 2) {:type ::other}))", "#r{:a 1, :b 2}"),
    ]);
}

#[test]
fn print_dup_methods_print_under_print_dup() {
    check_after(
        &[
            PRINT_METHODS[0],
            PRINT_METHODS[2],
            "(do (defmethod print-dup T [t w] (.write w \"#=(T. \") (print-dup (.-a t) w) (.write w \")\")) nil)",
        ],
        &[
            ("(binding [*print-dup* true] (pr-str [(T. \"a\")]))", "[#=(T. \"a\")]"),
            ("(pr-str [(T. \"a\")])", "[#t \"a\"]"),
            // Records print by their values under *print-dup*, not with print-method.
            ("(binding [*print-dup* true] (pr-str (->R 1 2)))", "#user.printing.R[1, 2]"),
        ],
    );
}

#[test]
fn print_dup_falls_back_to_print_method_for_core_types() {
    // Clojure's print-dup methods for vectors, maps, sets, seqs, symbols,
    // strings, keywords and the like call print-method.
    check_after(
        &[
            PRINT_METHODS[4],
            "(do (defmethod print-method (type :a) [k w] (.write w \"KW\")) nil)",
        ],
        &[
            ("(binding [*print-dup* true] (pr-str (with-meta [1] {:type ::k})))", "K:[1]"),
            ("(binding [*print-dup* true] (pr-str [:a #{:b} '(:c) 'x true \"s\" 1 nil]))", "[KW #{KW} (KW) x true \"s\" 1 nil]"),
            ("(binding [*print-dup* true] (pr-str (->R :d 2)))", "#user.printing.R[KW, 2]"),
        ],
    );
}

#[test]
fn functions_and_types_print_while_methods_exist() {
    // Suss can't take the type of a function or a type yet, so they print with
    // the built-in printer, as Clojure's methods for Fn and Class do.
    check_after(
        &[PRINT_METHODS[0], PRINT_METHODS[2], PRINT_METHODS[4]],
        &[
            ("(pr-str [(type 1) (type []) T])", "[#object[Number] suss.core.PersistentVector user.printing.T]"),
            ("(apply str (take 11 (pr-str [(T. inc)])))", "[#t #object"),
            ("(do (remove-method print-method :default) (pr-str [inc]))", "[#object[cljs$core$inc]]"),
        ],
    );
}

#[test]
fn print_method_defaults_and_removal_follow_clojure() {
    check_after(
        &[
            PRINT_METHODS[0],
            PRINT_METHODS[2],
            "(do (remove-method print-method T) nil)",
            "(do (defmethod print-method :default [x w] (.write w \"DEF\")) nil)",
        ],
        &[
            ("(apply str (take 23 (pr-str (T. 1))))", "#object[user.printing.T"),
            // A :default method prints a :type without a method, but no type's value.
            ("(pr-str (with-meta [1] {:type ::nope}))", "DEF"),
            ("(apply str (take 26 (pr-str [1 (T. 1)])))", "[1 #object[user.printing.T"),
            ("(do (remove-method print-method :default) (try (pr-str (with-meta [1] {:type ::nope})) (catch :default e (ex-message e))))",
             "No method in multimethod 'print-method' for dispatch value: :user.printing/nope"),
            ("(pr-str [1 :a])", "[1 :a]"),
        ],
    );
}

#[test]
fn print_readably_and_namespace_maps_follow_clojure() {
    check(&[
        ("(binding [*print-readably* false] (pr-str [\"a\\nb\" \\c]))", "[a\nb c]"),
        ("(print-str \"a\" [\"b\"])", "a [b]"),
        ("(binding [*print-namespace-maps* true] (pr-str {:a/b 1 :a/c 2}))", "#:a{:b 1, :c 2}"),
        ("(binding [*print-namespace-maps* true] (pr-str {'a/b 1 :a/c 2}))", "#:a{b 1, :c 2}"),
        ("(binding [*print-namespace-maps* true] (pr-str {:a/b 1 :c 2}))", "{:a/b 1, :c 2}"),
        ("(binding [*print-namespace-maps* true] (pr-str {:a/b 1 :b/c 2}))", "{:a/b 1, :b/c 2}"),
        ("(binding [*print-namespace-maps* true] (pr-str {}))", "{}"),
        ("(binding [*print-namespace-maps* true *print-length* 1] (pr-str {:a/b 1 :a/c 2}))", "#:a{:b 1, ...}"),
        ("(pr-str {:a/b 1 :a/c 2})", "{:a/b 1, :a/c 2}"),
        // Records never lift their keys.
        ("(binding [*print-namespace-maps* true] (pr-str (assoc (->R 1 2) :x/y 3)))", "#user.printing.R{:a 1, :b 2, :x/y 3}"),
    ]);
}

#[test]
fn print_dup_prints_everything_and_records_positionally() {
    check(&[
        // No length or level limit, and metadata always prints. Clojure prints
        // the metadata map as a #=(...) form; Suss prints it as print-method.
        ("(binding [*print-dup* true *print-length* 1 *print-level* 0] (pr-str [1 [2]]))", "[1 [2]]"),
        ("(binding [*print-dup* true] (pr-str (with-meta [1] {:k 1})))", "^{:k 1} [1]"),
        ("(binding [*print-dup* true *print-readably* false] (pr-str (with-meta [\"a\"] {:k 1})))", "^{:k 1} [\"a\"]"),
        // Records print their values, as Clojure's print-dup for IRecord.
        ("(binding [*print-dup* true] (pr-str (->R 1 \"x\")))", "#user.printing.R[1, \"x\"]"),
        ("(binding [*print-dup* true] (pr-str (assoc (->R 1 2) :c 3)))", "#user.printing.R[1, 2, 3]"),
        ("(binding [*print-dup* true *verbose-defrecords* true] (pr-str (->R 1 \"x\")))", "#user.printing.R{:a 1, :b \"x\"}"),
        ("(binding [*print-dup* true] (pr-str (read-string (binding [*print-dup* true] (pr-str (->R 1 [2]))))))", "#user.printing.R[1, [2]]"),
        // Clojure prints the array map as a #=(...) constructor call; Suss
        // prints it as print-method does, because it never evaluates while
        // reading. Lists and hash sets print as print-method in Clojure too.
        ("(binding [*print-dup* true] (pr-str [{:a 1} '(1) #{2}]))", "[{:a 1} (1) #{2}]"),
    ]);
}

#[test]
fn flush_on_newline_is_a_dynamic_option() {
    check(&[
        ("(pr-str *flush-on-newline*)", "true"),
        ("(binding [*flush-on-newline* false] (pr-str *flush-on-newline*))", "false"),
        ("(pr-str *verbose-defrecords*)", "false"),
    ]);
}

#[test]
fn components_print_with_options_and_no_hidden_imports() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("src/app")).unwrap();
    std::fs::write(
        root.path().join("src/app/printing.sus"),
        concat!(
            "(ns app.printing)\n",
            "(defrecord P [x y])\n",
            "(defn limited [] (binding [*print-length* 1 *print-meta* true] (pr-str [(with-meta [1 2] {:k 1})])))\n",
            "(defn levels [] (binding [*print-level* 1] (pr-str (->P [1] 2))))\n",
            "(defn dup [] (binding [*print-dup* true] (pr-str (->P 1 2))))\n",
            "(deftype T [a])\n",
            "(defmethod print-method T [t w] (.write w \"#t \") (print-method (.-a t) w))\n",
            "(defmethod print-method ::k [x w] (.write w \"K\"))\n",
            "(defn custom [] (binding [*print-length* 1] (pr-str [(T. [1 2]) (with-meta [] {:type ::k})])))\n",
        ),
    )
    .unwrap();
    std::fs::write(
        root.path().join("printing.wit"),
        "package test:printing;\nworld printing {\n export limited: func() -> string;\n export levels: func() -> string;\n export dup: func() -> string;\n export custom: func() -> string;\n}\n",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(root.path())
        .args(["compile", "-n", "app.printing", "--src", "src", "-w", "printing.wit", "--wit-world", "printing", "-o", "printing.wasm"])
        .arg("--compile-fuel")
        .arg(compilation_fuel::COMPILATION_FUEL.to_string())
        .args(["--export", "limited=app.printing/limited", "--export", "levels=app.printing/levels", "--export", "dup=app.printing/dup", "--export", "custom=app.printing/custom"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    // The world imports nothing, so neither does the component (spec §8).
    let wasm = std::fs::read(root.path().join("printing.wasm")).unwrap();
    let mut imports = 0;
    for payload in wasmparser::Parser::new(0).parse_all(&wasm) {
        if let wasmparser::Payload::ComponentImportSection(section) = payload.unwrap() {
            imports += section.count();
        }
    }
    assert_eq!(imports, 0, "printing must not add a hidden import");
    for (export, expected) in [
        ("limited", "[^{:k 1} [1 ...]]"),
        ("levels", "#app.printing.P{:x #, :y 2}"),
        ("dup", "#app.printing.P[1, 2]"),
        ("custom", "[#t [1 ...] ...]"),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_suss"))
            .current_dir(root.path())
            .args(["run", "printing.wasm", "--invoke", export])
            .output()
            .unwrap();
        assert!(output.status.success(), "{export}: {}", String::from_utf8_lossy(&output.stderr));
        let out = String::from_utf8(output.stdout).unwrap();
        let printed = match edn::read_one(out.trim(), &Options::default()) {
            Ok(Value::String(text)) => text,
            _ => out.trim().to_owned(),
        };
        assert_eq!(printed, expected, "{export}");
    }
}
