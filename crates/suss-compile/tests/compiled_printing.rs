//! Printing dependencies required by the public compiled-expression caller.
#![cfg(not(target_family = "wasm"))]
#[path = "support/portable_decode.rs"]
mod portable_decode;

use portable_decode::{Decoder, Observation};
use suss_compile::{portable_macros::CompiledMacros, portable_session::Session};

fn printing_fixture_expected(value: &serde_json::Value) -> Observation {
    match value["tag"].as_str().unwrap() {
        "nil" => Observation::Nil,
        "bool" => Observation::Bool(value["value"].as_bool().unwrap()),
        "string" => Observation::String(
            value["units"]
                .as_array()
                .unwrap()
                .iter()
                .map(|unit| u16::try_from(unit.as_u64().unwrap()).unwrap())
                .collect(),
        ),
        "vector" => Observation::Vector(
            value["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(printing_fixture_expected)
                .collect(),
        ),
        tag => panic!("unsupported printing fixture tag {tag}"),
    }
}

#[test]
fn pinned_keyword_conversions_and_map_writer_order_match_both_phases_after_gc() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/map-printing-cases.json"
    ))
    .unwrap();
    assert_eq!(corpus["schema"], 1);
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 4);
    assert_eq!(cases[0]["id"], "keyword-conversions");
    assert_eq!(cases[1]["id"], "map-writer-namespace-and-limit-order");
    assert_eq!(cases[2]["id"], "runtime-internal-string-nil-first");
    assert_eq!(cases[3]["id"], "map-lift-custom-cursor-order");
    assert_eq!(cases[0]["expected"]["items"].as_array().unwrap().len(), 15);
    assert_eq!(cases[1]["expected"]["items"].as_array().unwrap().len(), 8);
    for macro_phase in [false, true] {
        let mut session = if macro_phase {
            Session::new_macro().unwrap()
        } else {
            Session::new_repl().unwrap()
        };
        let mut decoder = Decoder::capture(&mut session, 100_000).unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        // Exercise private formatter dispatch from its actual core namespace.
        session.enter_namespace("suss.core").unwrap();
        for case in cases {
            let identity = case["id"].as_str().unwrap();
            let result = session
                .eval_with_macros(case["source"].as_str().unwrap(), &mut macros)
                .unwrap_or_else(|error| panic!("phase {macro_phase}, {identity}: {error}"));
            session.collect().unwrap();
            assert_eq!(
                decoder.decode_session(&mut session, &result).unwrap(),
                printing_fixture_expected(&case["expected"]),
                "phase {macro_phase}, {identity}"
            );
        }
    }
}

#[test]
fn pinned_source_function_names_preserve_scopes_capture_and_arities_after_gc() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/function-name-cases.json"
    ))
    .unwrap();
    assert_eq!(corpus["schema"], 1);
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 8);
    let ids = cases
        .iter()
        .map(|case| case["id"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(ids.len(), 8);
    for macro_phase in [false, true] {
        let mut session = if macro_phase {
            Session::new_macro().unwrap()
        } else {
            Session::new_repl().unwrap()
        };
        let mut decoder = Decoder::capture(&mut session, 100_000).unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        session
            .enter_namespace("suss-oracle.function-name-cases")
            .unwrap();
        for case in cases {
            let source = format!(
                "(defn observations [] {}) (observations)",
                case["source"].as_str().unwrap()
            );
            let result = session
                .eval_with_macros(&source, &mut macros)
                .unwrap_or_else(|error| panic!("phase {macro_phase}, {}: {error}", case["id"]));
            session.collect().unwrap();
            assert_eq!(
                decoder.decode_session(&mut session, &result).unwrap(),
                printing_fixture_expected(&case["expected"]),
                "phase {macro_phase}, {}",
                case["id"]
            );
            if case["id"] == "function-name-capture-redefinition-properties" {
                // Re-read and call actual captured closures after collection,
                // rather than only checking strings produced before collection.
                let retained = session.eval_with_macros(
                    "[(pr-str name-saved) (pr-str name-old) (pr-str (name-saved 2)) (pr-str (name-old 2))]", &mut macros).unwrap();
                session.collect().unwrap();
                assert_eq!(
                    decoder.decode_session(&mut session, &retained).unwrap(),
                    printing_fixture_expected(&case["expected"])
                );
            }
        }
    }
}

#[test]
fn printer_native_object_guard_uses_descriptor_identity_and_evaluates_once() {
    for macro_phase in [false, true] {
        let mut session = if macro_phase {
            Session::new_macro().unwrap()
        } else {
            Session::new_repl().unwrap()
        };
        let mut decoder = Decoder::capture(&mut session, 100_000).unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        let value = session
            .eval_with_macros(
                r#"
            (def factory (suss.bootstrap/object-factory))
            (def plain (factory))
            (deftype PrinterLookalike [contents])
            (def seen (atom 0))
            [(suss.bootstrap/native-object? plain)
             (suss.bootstrap/native-object? (suss.bootstrap/object-default-prototype))
             (suss.bootstrap/native-object? (do (swap! seen inc) plain))
             @seen
             (suss.bootstrap/native-object? nil)
             (suss.bootstrap/native-object? false)
             (suss.bootstrap/native-object? true)
             (suss.bootstrap/native-object? 42)
             (suss.bootstrap/native-object? "\ud800")
             (suss.bootstrap/native-object? (fn [] nil))
             (suss.bootstrap/native-object? (array 1))
             (suss.bootstrap/native-object? (PrinterLookalike. nil))
             (suss.bootstrap/native-object? [1])]
            "#,
                &mut macros,
            )
            .unwrap_or_else(|error| panic!("phase {macro_phase}: {error}"));
        session.collect().unwrap();
        let mut expected = (0..3).map(|_| Observation::Bool(true)).collect::<Vec<_>>();
        expected.push(Observation::Number(1f64.to_bits()));
        expected.extend((0..9).map(|_| Observation::Bool(false)));
        assert_eq!(
            decoder.decode_session(&mut session, &value).unwrap(),
            Observation::Vector(expected),
            "phase {macro_phase}"
        );
        let retained = session
            .eval_with_macros("(suss.bootstrap/native-object? plain)", &mut macros)
            .unwrap();
        session.collect().unwrap();
        assert_eq!(
            decoder.decode_session(&mut session, &retained).unwrap(),
            Observation::Bool(true)
        );
        // This is a storage guard, not the public constructor-based object?.
        // Shadowing a property must not change the native descriptor identity.
        let shadowed = session.eval_with_macros(
            "(suss.bootstrap/object-set plain \"constructor\" nil) (suss.bootstrap/native-object? plain)",
            &mut macros).unwrap();
        session.collect().unwrap();
        assert_eq!(
            decoder.decode_session(&mut session, &shadowed).unwrap(),
            Observation::Bool(true)
        );
        for source in [
            "(suss.bootstrap/native-object?)",
            "(suss.bootstrap/native-object? (swap! seen inc) nil)",
        ] {
            let error = session.eval_with_macros(source, &mut macros).unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("Invalid private native object adapter arity"),
                "phase {macro_phase}: {error}"
            );
        }
        let count = session.eval_with_macros("@seen", &mut macros).unwrap();
        session.collect().unwrap();
        assert_eq!(
            decoder.decode_session(&mut session, &count).unwrap(),
            Observation::Number(1f64.to_bits())
        );
    }
}

#[test]
fn printer_constructor_guard_tracks_owned_type_values_across_redefinition() {
    for macro_phase in [false, true] {
        let mut session = if macro_phase {
            Session::new_macro().unwrap()
        } else {
            Session::new_repl().unwrap()
        };
        let mut decoder = Decoder::capture(&mut session, 100_000).unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        let value = session
            .eval_with_macros(
                r#"
            (deftype PrinterConstructor [x])
            (def captured-constructor PrinterConstructor)
            (def old-instance (PrinterConstructor. 1))
            (defprotocol PrinterGuardProtocol (-guard-probe [this]))
            (def constructor-visits (atom 0))
            (set! (.-print_guard_probe captured-constructor) 1)
            [(suss.bootstrap/type-constructor? captured-constructor)
             (suss.bootstrap/type-constructor?
               (do (swap! constructor-visits inc) PrinterConstructor))
             @constructor-visits
             (suss.bootstrap/type-constructor? old-instance)
             (suss.bootstrap/type-constructor? PrinterGuardProtocol)
             (suss.bootstrap/type-constructor? (fn [] nil))
             (suss.bootstrap/type-constructor? (array 1))
             (suss.bootstrap/type-constructor? (js-obj))
             (suss.bootstrap/type-constructor? nil)
             (suss.bootstrap/type-constructor? true)
             (suss.bootstrap/type-constructor? 42)
             (suss.bootstrap/type-constructor? "\ud800")]
            "#,
                &mut macros,
            )
            .unwrap_or_else(|error| panic!("phase {macro_phase}: {error}"));
        session.collect().unwrap();
        let mut expected = (0..2).map(|_| Observation::Bool(true)).collect::<Vec<_>>();
        expected.push(Observation::Number(1f64.to_bits()));
        expected.extend((0..9).map(|_| Observation::Bool(false)));
        assert_eq!(
            decoder.decode_session(&mut session, &value).unwrap(),
            Observation::Vector(expected),
            "phase {macro_phase}"
        );
        let redefined = session
            .eval_with_macros(
                r#"
            (deftype PrinterConstructor [x y])
            [(suss.bootstrap/type-constructor? PrinterConstructor)
             (suss.bootstrap/type-constructor? captured-constructor)
             (instance? PrinterConstructor old-instance)]
            "#,
                &mut macros,
            )
            .unwrap();
        session.collect().unwrap();
        assert_eq!(
            decoder.decode_session(&mut session, &redefined).unwrap(),
            Observation::Vector(vec![
                Observation::Bool(true),
                Observation::Bool(true),
                Observation::Bool(false)
            ])
        );
    }
}

#[test]
fn public_object_predicate_observes_constructor_identity_and_shadowing() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/object-predicate-cases.json"
    ))
    .unwrap();
    assert_eq!(corpus["schema"], 1);
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 1);
    let case = &cases[0];
    assert_eq!(case["id"], "object-constructor-identity");
    assert_eq!(case["expected"]["tag"], "vector");
    let items = case["expected"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 17);
    for macro_phase in [false, true] {
        let mut session = if macro_phase {
            Session::new_macro().unwrap()
        } else {
            Session::new_repl().unwrap()
        };
        let mut decoder = Decoder::capture(&mut session, 100_000).unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        session
            .enter_namespace("suss-oracle.object-predicate-cases")
            .unwrap();
        let value = session
            .eval_with_macros(case["source"].as_str().unwrap(), &mut macros)
            .unwrap_or_else(|error| panic!("phase {macro_phase}: {error}"));
        session.collect().unwrap();
        let expected = items
            .iter()
            .map(|item| match item["tag"].as_str().unwrap() {
                "bool" => Observation::Bool(item["value"].as_bool().unwrap()),
                "f64" => Observation::Number(
                    u64::from_str_radix(item["bits"].as_str().unwrap(), 16).unwrap(),
                ),
                tag => panic!("unexpected object-predicate observation {tag}"),
            })
            .collect();
        assert_eq!(
            decoder.decode_session(&mut session, &value).unwrap(),
            Observation::Vector(expected),
            "phase {macro_phase}"
        );
    }
}

#[test]
fn shared_pinned_printing_corpus_matches_both_phases_after_gc() {
    assert_shared_printing_cases(0..16);
}

#[test]
fn original_anonymous_and_named_function_printing_match_both_phases_after_gc() {
    assert_shared_printing_cases(14..16);
}

#[test]
fn collection_printers_preserve_limits_metadata_and_effects_in_both_phases_after_gc() {
    // Run the unchanged collection fixture range independently so both Stores
    // are exercised even while later function-name printing is incomplete.
    assert_shared_printing_cases(7..14);
}

fn assert_shared_printing_cases(selected: std::ops::Range<usize>) {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/oracle/printing-cases.json")).unwrap();
    assert_eq!(corpus["schema"], 1);
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 16);
    let identities = cases
        .iter()
        .map(|case| case["id"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(identities.len(), cases.len(), "duplicate printing case ID");
    for macro_phase in [false, true] {
        let mut session = if macro_phase {
            Session::new_macro().unwrap()
        } else {
            Session::new_repl().unwrap()
        };
        let mut decoder = Decoder::capture(&mut session, 100_000).unwrap();
        // The primary runner evaluates expressions inside this actual namespace
        // and function. Match that source context, including observable fn names.
        session
            .enter_namespace("suss-oracle.printing-cases")
            .unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        for case in &cases[selected.clone()] {
            let identity = case["id"].as_str().unwrap();
            let source = case["source"].as_str().unwrap();
            assert_eq!(case["expected"]["tag"], "string", "{identity}");
            let expected = case["expected"]["units"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| u16::try_from(value.as_u64().unwrap()).unwrap())
                .collect::<Vec<_>>();
            let source = format!("(defn observations [] {source}) (observations)");
            let value = session
                .eval_with_macros(&source, &mut macros)
                .unwrap_or_else(|error| panic!("phase {macro_phase}, {identity}: {error}"));
            session.collect().unwrap();
            assert_eq!(
                decoder.decode_session(&mut session, &value).unwrap(),
                Observation::String(expected),
                "phase {macro_phase}, {identity}"
            );
        }
    }
}

#[test]
fn writer_protocol_orders_effects_and_restores_print_options_after_throw() {
    for macro_phase in [false, true] {
        let mut session = if macro_phase {
            Session::new_macro().unwrap()
        } else {
            Session::new_repl().unwrap()
        };
        let mut decoder = Decoder::capture(&mut session, 100_000).unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        let value = session
            .eval_with_macros(
                r#"
          (def writes (atom []))
          (deftype RecordingWriter [entries]
            IWriter
            (-write [this text]
              (swap! entries conj text)
              (if (= text "stop") (throw 17) this))
            (-flush [_] (swap! entries conj "flush")))
          (def writer (RecordingWriter. writes))
          (write-all writer "a" "b")
          (try (write-all writer "stop" "unreached") (catch :default error nil))
          (write-all writer "c")
          (-flush writer)
          (def nested
            (binding [*print-readably* false *print-meta* true *print-dup* true
                      *print-length* 2 *print-level* 1 *print-fn-bodies* true]
              (try (throw [*print-readably* *print-meta* *print-dup*
                           *print-length* *print-level* *print-fn-bodies*])
                   (catch :default error error))))
          [@writes nested
           [*print-readably* *print-meta* *print-dup* *print-length*
            *print-level* *print-fn-bodies*]]
        "#,
                &mut macros,
            )
            .unwrap_or_else(|error| panic!("phase {macro_phase}: {error}"));
        session.collect().unwrap();
        assert_eq!(
            decoder.decode_session(&mut session, &value).unwrap(),
            Observation::Vector(vec![
                Observation::Vector(
                    ["a", "b", "stop", "c", "flush"]
                        .into_iter()
                        .map(|text| Observation::String(text.encode_utf16().collect()))
                        .collect()
                ),
                Observation::Vector(vec![
                    Observation::Bool(false),
                    Observation::Bool(true),
                    Observation::Bool(true),
                    Observation::Number(2f64.to_bits()),
                    Observation::Number(1f64.to_bits()),
                    Observation::Bool(true)
                ]),
                Observation::Vector(vec![
                    Observation::Bool(true),
                    Observation::Bool(false),
                    Observation::Bool(false),
                    Observation::Nil,
                    Observation::Nil,
                    Observation::Bool(false)
                ])
            ]),
            "phase {macro_phase}"
        );
    }
}

#[test]
fn string_buffer_writer_preserves_utf16_and_returns_the_buffer() {
    for macro_phase in [false, true] {
        let mut session = if macro_phase {
            Session::new_macro().unwrap()
        } else {
            Session::new_repl().unwrap()
        };
        let mut decoder = Decoder::capture(&mut session, 100_000).unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        let value = session
            .eval_with_macros(
                r#"
          (def buffer (BootstrapStringBuffer. ""))
          (def writer (StringBufferWriter. buffer))
          (def empty-result (write-all writer))
          (def append-result (-write writer "a\ud800"))
          (write-all writer "🙂" "z")
          [empty-result (identical? append-result buffer) (-flush writer)
           (.toString buffer)]
        "#,
                &mut macros,
            )
            .unwrap_or_else(|error| panic!("phase {macro_phase}: {error}"));
        session.collect().unwrap();
        assert_eq!(
            decoder.decode_session(&mut session, &value).unwrap(),
            Observation::Vector(vec![
                Observation::Nil,
                Observation::Bool(true),
                Observation::Nil,
                Observation::String(vec![97, 0xd800, 0xd83d, 0xde42, 122])
            ]),
            "phase {macro_phase}"
        );
    }
}

#[test]
fn quoted_string_escapes_all_pinned_units_without_losing_surrogates() {
    for macro_phase in [false, true] {
        let mut session = if macro_phase {
            Session::new_macro().unwrap()
        } else {
            Session::new_repl().unwrap()
        };
        let mut decoder = Decoder::capture(&mut session, 100_000).unwrap();
        // This test inspects the retained private printer dependency in its own
        // namespace, rather than exposing it as a public API.
        session.enter_namespace("suss.core").unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        let value = session
            .eval_with_macros(r#"(quote-string "\"\\\b\f\n\r\t\ud800🙂")"#, &mut macros)
            .unwrap_or_else(|error| panic!("phase {macro_phase}: {error}"));
        session.collect().unwrap();
        assert_eq!(
            decoder.decode_session(&mut session, &value).unwrap(),
            Observation::String(vec![
                34, 92, 34, 92, 92, 92, 98, 92, 102, 92, 110, 92, 114, 92, 116, 0xd800, 0xd83d,
                0xde42, 34
            ]),
            "phase {macro_phase}"
        );
    }
}

#[test]
fn sequential_writer_preserves_limits_and_does_not_print_truncated_items() {
    for macro_phase in [false, true] {
        let mut session = if macro_phase {
            Session::new_macro().unwrap()
        } else {
            Session::new_repl().unwrap()
        };
        let mut decoder = Decoder::capture(&mut session, 100_000).unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        let value = session
            .eval_with_macros(
                r#"
          (def printed (atom []))
          (def render (fn [opts]
            (let [buffer (BootstrapStringBuffer. "")
                  writer (StringBufferWriter. buffer)]
              (pr-sequential-writer writer
                (fn [item writer opts] (swap! printed conj item) (-write writer item))
                "[" " " "]" opts ["a" "b" "c"])
              (.toString buffer))))
          [(render nil)
           (binding [*print-length* 0] (render nil))
           (binding [*print-length* 2] (render nil))
           (binding [*print-level* 0] (render nil))
           (render {:print-length 1 :more-marker "+"})
           @printed *print-level* *print-length*]
        "#,
                &mut macros,
            )
            .unwrap_or_else(|error| panic!("phase {macro_phase}: {error}"));
        session.collect().unwrap();
        let string = |text: &str| Observation::String(text.encode_utf16().collect());
        assert_eq!(
            decoder.decode_session(&mut session, &value).unwrap(),
            Observation::Vector(vec![
                string("[a b c]"),
                string("[...]"),
                string("[a b ...]"),
                string("#"),
                string("[a +]"),
                Observation::Vector(
                    ["a", "b", "c", "a", "b", "a"]
                        .into_iter()
                        .map(string)
                        .collect()
                ),
                Observation::Nil,
                Observation::Nil
            ]),
            "phase {macro_phase}"
        );
    }
}

#[test]
fn alternate_writer_runs_once_per_object_and_keeps_separator_order() {
    for macro_phase in [false, true] {
        let mut session = if macro_phase {
            Session::new_macro().unwrap()
        } else {
            Session::new_repl().unwrap()
        };
        let mut decoder = Decoder::capture(&mut session, 100_000).unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        let value = session
            .eval_with_macros(
                r#"
          (def visited (atom []))
          (def alternate (fn [object writer opts]
            (swap! visited conj object)
            (-write writer (if (= object 1) "first" "second"))))
          [(pr-str) (pr-str-with-opts [1 2] {:alt-impl alternate}) @visited]
        "#,
                &mut macros,
            )
            .unwrap_or_else(|error| panic!("phase {macro_phase}: {error}"));
        session.collect().unwrap();
        assert_eq!(
            decoder.decode_session(&mut session, &value).unwrap(),
            Observation::Vector(vec![
                Observation::String(vec![]),
                Observation::String("first second".encode_utf16().collect()),
                Observation::Vector(vec![
                    Observation::Number(1f64.to_bits()),
                    Observation::Number(2f64.to_bits())
                ])
            ]),
            "phase {macro_phase}"
        );
    }
}

#[test]
fn retained_printer_source_facts_fit_the_bounded_macro_environment() {
    for macro_phase in [false, true] {
        let mut session = if macro_phase {
            Session::new_macro().unwrap()
        } else {
            Session::new_repl().unwrap()
        };
        let mut decoder = Decoder::capture(&mut session, 100_000).unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        // The second definition transports the actual first initializer and the
        // retained core's source-function facts. Neither unfinished printer runs.
        let value = session
            .eval_with_macros(
                r#"
          (defn first-print [] (pr-str))
          (defn second-print [] (pr-str nil true false))
          [(js-fn? first-print) (js-fn? second-print)]
        "#,
                &mut macros,
            )
            .unwrap_or_else(|error| panic!("phase {macro_phase}: {error}"));
        session.collect().unwrap();
        assert_eq!(
            decoder.decode_session(&mut session, &value).unwrap(),
            Observation::Vector(vec![Observation::Bool(true), Observation::Bool(true)]),
            "phase {macro_phase}"
        );
    }
}
