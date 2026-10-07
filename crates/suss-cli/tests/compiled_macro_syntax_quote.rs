//! Executing M3 syntax quote regressions. These exercise compiled transformers,
//! rather than the prototype evaluator or reader encoding alone.
use suss_cli::{portable_macros::CompiledMacros, portable_session::Session};
use wasmtime::Val;

fn number(runtime: &mut Session, macros: &mut CompiledMacros, source: &str) -> f64 {
    let value = runtime.eval_with_macros(source, macros).unwrap();
    runtime.collect().unwrap();
    runtime
        .inspect(&value, |mut store, value| {
            let fields = value
                .unwrap_anyref()
                .unwrap()
                .as_struct(&store)?
                .unwrap()
                .fields(&mut store)?
                .collect::<Vec<_>>();
            let [Val::F64(bits)] = fields.as_slice() else {
                panic!("Expected numeric execution result: {fields:?}")
            };
            Ok(f64::from_bits(*bits))
        })
        .unwrap()
}

#[test]
fn compiled_macro_syntax_quote_executes_unquote_and_sequence_splicing() {
    let mut macros = CompiledMacros::new().unwrap();
    macros
        .define(include_str!(
            "../../../tests/oracle/syntax-quote-add-one.sus"
        ))
        .unwrap();
    macros
        .define(include_str!(
            "../../../tests/oracle/syntax-quote-sum-inputs.sus"
        ))
        .unwrap();
    let mut runtime = Session::new_repl().unwrap();
    assert_eq!(
        number(
            &mut runtime,
            &mut macros,
            "(let [x 40] (add-one (add-one x)))"
        ),
        42.0
    );
    assert_eq!(
        number(&mut runtime, &mut macros, "(sum-inputs 20 22)"),
        42.0
    );
    assert_eq!(number(&mut runtime, &mut macros, "(sum-inputs)"), 0.0);
}

#[test]
fn compiled_macro_syntax_quote_matches_actual_pinned_execution_and_reader_form() {
    use suss_cli::portable_macro_data::FormBridge;
    use suss_reader::forms::Kind;
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/syntax-quote-observations.json"
    ))
    .unwrap();
    assert_eq!(corpus["schema"], 1);
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 1);
    assert_eq!(cases[0][0], "execution");
    let expected = cases[0][1].as_array().unwrap();
    assert_eq!(expected.len(), 18);
    for mut runtime in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        for source in [
            include_str!("../../../tests/oracle/syntax-quote-add-one.sus"),
            include_str!("../../../tests/oracle/syntax-quote-sum-inputs.sus"),
            include_str!("../../../tests/oracle/syntax-quote-twice.sus"),
            include_str!("../../../tests/oracle/syntax-quote-data.sus"),
            include_str!("../../../tests/oracle/syntax-quote-vector-splice.sus"),
            include_str!("../../../tests/oracle/syntax-quote-inspect-reader.sus"),
        ] {
            macros.define(source).unwrap();
        }
        runtime.set_operation_fuel(50_000_000);
        runtime
            .eval_with_macros(
                include_str!("../../../tests/oracle/syntax-quote-fixture.sus"),
                &mut macros,
            )
            .unwrap();
        let value = runtime
            .eval_with_macros("(syntax-quote-observations)", &mut macros)
            .unwrap();
        runtime.collect().unwrap();
        let bridge = FormBridge::new(&mut runtime).unwrap();
        let actual = bridge.read(&mut runtime, &value, 0..28).unwrap();
        let Kind::List(actual) = actual.kind else {
            panic!("Expected independently decoded observations")
        };
        assert_eq!(actual.len(), expected.len());
        for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
            let Kind::Number(actual) = actual.kind else {
                panic!("Expected numeric observation {index}")
            };
            assert_eq!(
                actual.to_bits(),
                expected.as_f64().unwrap().to_bits(),
                "Observation {index}"
            );
        }
    }
}

#[test]
fn compiled_macro_syntax_quote_reuses_auto_gensyms_and_evaluates_input_once() {
    let mut macros = CompiledMacros::new().unwrap();
    macros
        .define(include_str!("../../../tests/oracle/syntax-quote-twice.sus"))
        .unwrap();
    let mut runtime = Session::new_repl().unwrap();
    runtime.eval("(def effects 0)").unwrap();
    assert_eq!(
        number(
            &mut runtime,
            &mut macros,
            "(let [value 100] (twice (do (set! effects (+ effects 1)) 21)))"
        ),
        42.0
    );
    assert_eq!(number(&mut runtime, &mut macros, "effects"), 1.0);
    assert_eq!(
        number(&mut runtime, &mut macros, "(twice (twice 10.5))"),
        42.0
    );
}

#[test]
fn compiled_macro_reader_provenance_retains_automatic_parent_positions() {
    use suss_cli::portable_macro_data::FormBridge;
    use suss_reader::forms::Kind;
    for mut runtime in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        macros
            .define(include_str!(
                "../../../tests/oracle/syntax-quote-inspect-reader.sus"
            ))
            .unwrap();
        let source = "; 😀\r\n(inspect-reader `(+ 20 22))";
        let value = runtime.eval_with_macros(source, &mut macros).unwrap();
        runtime.collect().unwrap();
        let bridge = FormBridge::new(&mut runtime).unwrap();
        let actual = bridge.read(&mut runtime, &value, 0..source.len()).unwrap();
        let Kind::Vector(items) = actual.kind else {
            panic!("reader observations")
        };
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].kind, Kind::Number(2.0));
        assert_eq!(
            items[1].kind,
            Kind::String("clojure.core".encode_utf16().collect())
        );
        assert_eq!(
            items[2].kind,
            Kind::String("sequence".encode_utf16().collect())
        );
    }
}

#[test]
fn syntax_quote_state_survives_fragments_and_failed_input_does_not_advance_it() {
    use suss_cli::portable_macro_data::FormBridge;
    use suss_reader::forms::Kind;
    for mut runtime in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let bridge = FormBridge::new(&mut runtime).unwrap();
        let first = runtime.eval("`value#").unwrap();
        let first = bridge.read(&mut runtime, &first, 0..7).unwrap();
        assert!(runtime.eval("`[value# ~unresolved]").is_err());
        let second = runtime.eval("`value#").unwrap();
        let second = bridge.read(&mut runtime, &second, 0..7).unwrap();
        let (Kind::Symbol(first), Kind::Symbol(second)) = (first.kind, second.kind) else {
            panic!("generated symbols")
        };
        fn id(name: &str) -> u64 {
            name.strip_suffix("__auto__")
                .unwrap()
                .rsplit_once("__")
                .unwrap()
                .1
                .parse()
                .unwrap()
        }
        assert_eq!(
            id(&second.name),
            id(&first.name) + 2,
            "Failed compile must not consume a reader ID"
        );
    }
}

#[test]
fn compiled_macro_syntax_quote_preserves_collection_data_without_reexecution() {
    let mut macros = CompiledMacros::new().unwrap();
    macros
        .define(include_str!("../../../tests/oracle/syntax-quote-data.sus"))
        .unwrap();
    macros
        .define(include_str!(
            "../../../tests/oracle/syntax-quote-vector-splice.sus"
        ))
        .unwrap();
    for mut runtime in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        runtime.eval("(def effects 0)").unwrap();
        assert_eq!(
            number(
                &mut runtime,
                &mut macros,
                "(first (get (quoted-data 42) :a))"
            ),
            42.0
        );
        assert_eq!(
            number(
                &mut runtime,
                &mut macros,
                "(if (contains? (get (quoted-data 42) :b) 42) 42 0)"
            ),
            42.0
        );
        assert_eq!(
            number(
                &mut runtime,
                &mut macros,
                "(count (first (get (quoted-data (set! effects 99)) :a)))"
            ),
            3.0
        );
        assert_eq!(number(&mut runtime, &mut macros, "effects"), 0.0);
        assert_eq!(
            number(
                &mut runtime,
                &mut macros,
                "(let [v (quoted-vector 0 [20 22])] (+ (nth v 0) (nth v 1) (nth v 2)))"
            ),
            42.0
        );
        assert_eq!(
            number(&mut runtime, &mut macros, "(count (quoted-vector 42 []))"),
            1.0
        );
    }
}

#[test]
fn compiled_macro_lazy_concat_output_is_executed_as_source_forms() {
    for mut runtime in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        macros
            .define("(defmacro delayed-add [x] (concat (list '+) (list x 1)))")
            .unwrap();
        assert_eq!(number(&mut runtime, &mut macros, "(delayed-add 41)"), 42.0);
    }
}

#[test]
fn compiled_macro_lazy_chunked_concat_output_is_executed_as_source_forms() {
    for mut runtime in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        macros
            .define("(defmacro chunked-add [x] (concat ['+ x] (list 1)))")
            .unwrap();
        assert_eq!(number(&mut runtime, &mut macros, "(chunked-add 41)"), 42.0);
    }
}

#[test]
fn compiled_macro_lazy_output_rejects_cycles_and_recovers_without_publication() {
    use suss_cli::portable_session::SessionError;
    let mut macros = CompiledMacros::new().unwrap();
    macros.define("(defmacro cycle-data [] (let [value (new cljs.core/LazySeq nil nil nil nil)] (set! (.-s value) value) value))").unwrap();
    macros
        .define("(defmacro throw-data [] (new cljs.core/LazySeq nil (fn [] (throw 7)) nil nil))")
        .unwrap();
    macros.define("(defmacro bad-chunk [] (new cljs.core/ChunkedCons (new cljs.core/ArrayChunk (array 1) 0 99) nil nil nil))").unwrap();
    macros.define("(defmacro empty-chunk [] (new cljs.core/ChunkedCons (new cljs.core/ArrayChunk (array) 0 0) nil nil nil))").unwrap();
    macros.define("(defmacro next-value [] 42)").unwrap();
    for mut runtime in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        runtime.eval("(def stable 7)").unwrap();
        for source in [
            "(def ghost 9) (cycle-data)",
            "(def ghost 9) (throw-data)",
            "(def ghost 9) (bad-chunk)",
            "(def ghost 9) (empty-chunk)",
        ] {
            let before = runtime.stats();
            let Err(SessionError::Compile(error)) = runtime.eval_with_macros(source, &mut macros)
            else {
                panic!("Malformed/deferred macro data must fail compilation")
            };
            assert_eq!(error.span.start, 14);
            assert_eq!(runtime.stats(), before);
            assert!(runtime.eval("ghost").is_err());
            assert_eq!(number(&mut runtime, &mut macros, "stable"), 7.0);
            assert_eq!(number(&mut runtime, &mut macros, "(next-value)"), 42.0);
        }
    }
}

#[test]
fn compiled_macro_lazy_metadata_is_validated_before_thunk_effects() {
    let mut macros = CompiledMacros::new().unwrap();
    macros
        .define("(defmacro initialize [] (def touched 0) nil)")
        .unwrap();
    macros.define("(defmacro bad-metadata [] (new cljs.core/LazySeq 7 (fn [] (do (set! touched 99) (list 42))) nil nil))").unwrap();
    macros
        .define("(defmacro touched-value [] touched)")
        .unwrap();
    let mut runtime = Session::new_repl().unwrap();
    runtime
        .eval_with_macros("(initialize)", &mut macros)
        .unwrap();
    assert!(
        runtime
            .eval_with_macros("(bad-metadata)", &mut macros)
            .is_err()
    );
    assert_eq!(number(&mut runtime, &mut macros, "(touched-value)"), 0.0);
}

#[test]
fn compiled_macro_lazy_undefined_sequence_tail_is_empty() {
    let mut macros = CompiledMacros::new().unwrap();
    macros
        .define("(defmacro absent-data [] (concat (aget (array) 0)))")
        .unwrap();
    for mut runtime in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        assert_eq!(
            number(&mut runtime, &mut macros, "(count (absent-data))"),
            0.0
        );
    }
}

#[test]
fn compiled_syntax_quote_splices_inside_defn_helpers_of_macro_namespaces() {
    // The helper's body passes through the compiled defn macro, so its
    // syntax-quote output reaches analysis as reader data naming clojure.core.
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("demo")).unwrap();
    std::fs::write(
        root.path().join("demo/helpers.cljc"),
        "(ns demo.helpers)\n(defn helper [xs] `(list ~@xs))\n(defmacro via [& xs] (helper xs))\n",
    )
    .unwrap();
    std::fs::write(
        root.path().join("demo/use.cljc"),
        "(ns demo.use (:require-macros [demo.helpers :refer [via]]))\n(def total (count (via 1 2 3)))\n(def qualified (clojure.core/inc 41))\n",
    )
    .unwrap();
    let mut session = suss_cli::portable_session::Session::with_options(
        suss_cli::portable_session::SessionOptions {
            source_paths: vec![root.path().to_owned()],
            ..Default::default()
        },
    )
    .unwrap();
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    let mut macros = suss_cli::portable_macros::CompiledMacros::new().unwrap();
    session
        .load_namespace_with_macros("demo.use", &mut macros)
        .unwrap();
    for (name, expected) in [("demo.use/total", 3.0), ("demo.use/qualified", 42.0)] {
        let value = session.eval(name).unwrap();
        let bits = session
            .inspect(&value, |mut store, value| {
                let fields = value
                    .unwrap_anyref()
                    .unwrap()
                    .as_struct(&store)?
                    .unwrap()
                    .fields(&mut store)?
                    .collect::<Vec<_>>();
                let [wasmtime::Val::F64(bits)] = fields.as_slice() else {
                    panic!("Number");
                };
                Ok(*bits)
            })
            .unwrap();
        assert_eq!(f64::from_bits(bits), expected, "{name}");
    }
}
