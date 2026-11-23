//! Executing foundations for migrating the complete expression corpus to ABI2.
#![cfg(not(target_family = "wasm"))]
#[path = "support/portable_decode.rs"]
mod portable_decode;
use portable_decode::{Decoder, Observation, ProbeEval};
use suss_compile::portable_session::Session;
use wasmtime::{AnyRef, I31, Val};

#[test]
fn scalar_decoder_preserves_bits_and_language_sentinels_after_gc() {
    let mut session = Session::new_repl().unwrap();
    for (source, expected) in [
        ("nil", Observation::Nil),
        ("false", Observation::Bool(false)),
        ("true", Observation::Bool(true)),
        ("42", Observation::Long(42)),
        ("42.0", Observation::Number(42.0_f64.to_bits())),
        ("9007199254740993", Observation::Long(9007199254740993)),
        ("-0.0", Observation::Number((-0.0_f64).to_bits())),
        ("##NaN", Observation::Number(f64::NAN.to_bits())),
        ("##Inf", Observation::Number(f64::INFINITY.to_bits())),
        ("##-Inf", Observation::Number(f64::NEG_INFINITY.to_bits())),
    ] {
        let value = session.eval_probe(source).unwrap();
        session.collect().unwrap();
        let observed = session
            .inspect(&value, |mut store, value| {
                Decoder::new(16).decode(&mut store, &value)
            })
            .unwrap();
        assert_eq!(observed, expected, "{source}");
    }
}

#[test]
fn string_decoder_preserves_utf16_including_astral_and_lone_surrogates() {
    let mut session = Session::new_repl().unwrap();
    let value = session.eval_probe(r#""A😀\uD800x\uDC00λ""#).unwrap();
    session.collect().unwrap();
    let observed = session
        .inspect(&value, |mut store, value| {
            Decoder::new(32).decode(&mut store, &value)
        })
        .unwrap();
    assert_eq!(
        observed,
        Observation::String(vec![65, 0xd83d, 0xde00, 0xd800, 120, 0xdc00, 0x3bb])
    );
}

#[test]
fn decoder_rejects_unknown_sentinels_nulls_and_unboxed_values() {
    let mut session = Session::new_repl().unwrap();
    let value = session.eval_probe("nil").unwrap();
    session
        .inspect(&value, |mut store, _| {
            // ABI v3: a sentinel is a one-field i32 struct; forge unknown tags.
            let sentinel_shape = wasmtime::StructType::new(
                store.engine(),
                [wasmtime::FieldType::new(
                    wasmtime::Mutability::Const,
                    wasmtime::StorageType::ValType(wasmtime::ValType::I32),
                )],
            )?;
            let allocator = wasmtime::StructRefPre::new(&mut store, sentinel_shape);
            for tag in [1, 6, 99] {
                let forged = wasmtime::StructRef::new(&mut store, &allocator, &[Val::I32(tag)])?;
                let forged = Val::AnyRef(Some(forged.to_anyref()));
                assert!(
                    Decoder::new(16)
                        .decode(&mut store, &forged)
                        .unwrap_err()
                        .to_string()
                        .contains("sentinel")
                );
            }
            // Every i31 is a long (ABI v3), never a sentinel.
            for tag in [0, 1, 2, 4, 6, 99] {
                let i31 = Val::AnyRef(Some(AnyRef::from_i31(
                    &mut store,
                    I31::new_u32(tag).unwrap(),
                )));
                assert_eq!(
                    Decoder::new(16).decode(&mut store, &i31)?,
                    Observation::Long(i64::from(tag))
                );
            }
            // A null eqref is language nil under ABI v3, not a malformed value.
            assert_eq!(
                Decoder::new(16).decode(&mut store, &Val::null_any_ref())?,
                Observation::Nil
            );
            for malformed in [
                Val::I32(42),
                Val::F64(42.0_f64.to_bits()),
            ] {
                assert!(
                    Decoder::new(16)
                        .decode(&mut store, &malformed)
                        .unwrap_err()
                        .to_string()
                        .contains("language value")
                );
            }
            Ok(())
        })
        .unwrap();
}

#[test]
fn unknown_nominal_objects_and_raw_argument_arrays_are_decode_errors() {
    let mut session = Session::new_repl().unwrap();
    for source in [
        "(deftype DecoderUnknown [x]) (new DecoderUnknown 42)",
        "(array 1 2)",
    ] {
        let value = session.eval_probe(source).unwrap();
        session
            .inspect(&value, |mut store, value| {
                assert!(Decoder::new(32).decode(&mut store, &value).is_err());
                Ok(())
            })
            .unwrap();
    }
}

#[test]
fn decoder_bounds_are_checked_before_string_traversal() {
    let mut session = Session::new_repl().unwrap();
    let value = session.eval_probe(r#""abcdef""#).unwrap();
    session
        .inspect(&value, |mut store, value| {
            assert!(
                Decoder::new(6)
                    .decode(&mut store, &value)
                    .unwrap_err()
                    .to_string()
                    .contains("traversal limit")
            );
            assert_eq!(
                Decoder::new(7).decode(&mut store, &value)?,
                Observation::String("abcdef".encode_utf16().collect())
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn identifier_decoder_retains_canonical_identity_through_gc_and_redefinition() {
    let mut session = Session::new_repl().unwrap();
    let mut decoder = Decoder::capture(&mut session, 4096).unwrap();
    for (source, expected) in [
        (
            ":ready",
            Observation::Keyword(None, "ready".encode_utf16().collect()),
        ),
        (
            ":app/λ",
            Observation::Keyword(
                Some("app".encode_utf16().collect()),
                "λ".encode_utf16().collect(),
            ),
        ),
        (
            "'answer",
            Observation::Symbol(None, "answer".encode_utf16().collect()),
        ),
        (
            "'app/answer",
            Observation::Symbol(
                Some("app".encode_utf16().collect()),
                "answer".encode_utf16().collect(),
            ),
        ),
    ] {
        let value = session.eval_probe(source).unwrap();
        session.collect().unwrap();
        assert_eq!(
            decoder.decode_session(&mut session, &value).unwrap(),
            expected
        );
    }
    let retained = session.eval_probe(":ready").unwrap();
    let impostor = session
        .eval(r#"(deftype Keyword [ns name fqn hash]) (new Keyword nil "ready" "ready" nil)"#)
        .unwrap();
    assert!(
        decoder
            .decode_session(&mut session, &impostor)
            .unwrap_err()
            .to_string()
            .contains("Unsupported ABI2 value layout")
    );
    session.eval_probe("(ns suss.core) (def Keyword nil)").unwrap();
    session.collect().unwrap();
    assert_eq!(
        decoder.decode_session(&mut session, &retained).unwrap(),
        Observation::Keyword(None, "ready".encode_utf16().collect())
    );
}

#[test]
fn captured_decoder_rejects_foreign_and_reset_sessions_before_descriptor_access() {
    use suss_compile::portable_session::SessionError;
    let mut session = Session::new_repl().unwrap();
    let mut decoder = Decoder::capture(&mut session, 4096).unwrap();
    let mut other = Session::new_repl().unwrap();
    let foreign = other.eval(":ready").unwrap();
    assert!(matches!(
        decoder.decode_session(&mut other, &foreign),
        Err(SessionError::ForeignValue)
    ));
    let old = session.eval_probe(":ready").unwrap();
    session.reset().unwrap();
    assert!(matches!(
        decoder.decode_session(&mut session, &old),
        Err(SessionError::ForeignValue)
    ));
}

#[test]
fn canonical_identifiers_reject_malformed_namespaces_names_and_excessive_nesting() {
    let mut session = Session::new_repl().unwrap();
    let mut decoder = Decoder::capture(&mut session, 4096).unwrap();
    for (source, expected_error) in [
        (
            r#"(new suss.core/Keyword 42 "x" "x" nil)"#,
            "Malformed identifier namespace",
        ),
        (
            r#"(new suss.core/Keyword nil 42 "x" nil)"#,
            "Malformed identifier name",
        ),
        (
            r#"(loop [i 0 ns nil] (if (< i 130) (recur (inc i) (new suss.core/Keyword ns "x" "x" nil)) ns))"#,
            "ABI2 decoder nesting limit",
        ),
    ] {
        let value = session.eval_probe(source).unwrap();
        assert!(
            decoder
                .decode_session(&mut session, &value)
                .unwrap_err()
                .to_string()
                .contains(expected_error)
        );
    }
}

#[test]
fn persistent_vectors_decode_tree_boundaries_and_nested_values_without_guest_calls() {
    let mut session = Session::new_repl().unwrap();
    // Bound the intentionally large fixture construction separately from the
    // decoder traversal budget; default interactive fuel is only10million.
    session.set_operation_fuel(1_000_000_000);
    let mut decoder = Decoder::capture(&mut session, 100_000).unwrap();
    for count in [0, 1, 31, 32, 33, 1024, 1025, 1057] {
        let value = session
            .eval_probe(&format!(
                "(loop [i 0 xs []] (if (= i {count}) xs (recur (inc i) (conj xs i))))"
            ))
            .unwrap();
        session.collect().unwrap();
        assert_eq!(
            decoder.decode_session(&mut session, &value).unwrap(),
            Observation::Vector(
                (0..count)
                    .map(|i| Observation::Long(i as i64))
                    .collect()
            ),
            "vector count={count}"
        );
    }
    let value = session
        .eval(r#"[nil true -0.0 "😀" :app/ready 'app/name [42]]"#)
        .unwrap();
    session.collect().unwrap();
    assert_eq!(
        decoder.decode_session(&mut session, &value).unwrap(),
        Observation::Vector(vec![
            Observation::Nil,
            Observation::Bool(true),
            Observation::Number((-0.0_f64).to_bits()),
            Observation::String(vec![0xd83d, 0xde00]),
            Observation::Keyword(
                Some("app".encode_utf16().collect()),
                "ready".encode_utf16().collect()
            ),
            Observation::Symbol(
                Some("app".encode_utf16().collect()),
                "name".encode_utf16().collect()
            ),
            Observation::Vector(vec![Observation::Long(42)]),
        ])
    );
    let retained = session.eval_probe("[42]").unwrap();
    session
        .eval("(ns suss.core) (def PersistentVector nil) (def VectorNode nil)")
        .unwrap();
    session.collect().unwrap();
    assert_eq!(
        decoder.decode_session(&mut session, &retained).unwrap(),
        Observation::Vector(vec![Observation::Long(42)])
    );
}

#[test]
fn persistent_lists_and_cons_decode_empty_nested_and_gc_retained_values() {
    let mut session = Session::new_repl().unwrap();
    let mut decoder = Decoder::capture(&mut session, 100_000).unwrap();
    for (source, expected) in [
        ("'()", Observation::List(vec![])),
        (
            "'(1 2 3)",
            Observation::List(
                vec![1_i64, 2, 3]
                    .into_iter()
                    .map(Observation::Long)
                    .collect(),
            ),
        ),
        (
            "(cons 1 (list 2 3))",
            Observation::List(
                vec![1_i64, 2, 3]
                    .into_iter()
                    .map(Observation::Long)
                    .collect(),
            ),
        ),
        (
            "(new suss.core/Cons nil 1 [2 3] nil)",
            Observation::List(
                vec![1_i64, 2, 3]
                    .into_iter()
                    .map(Observation::Long)
                    .collect(),
            ),
        ),
        (
            "(list [42] (list nil false))",
            Observation::List(vec![
                Observation::Vector(vec![Observation::Long(42)]),
                Observation::List(vec![Observation::Nil, Observation::Bool(false)]),
            ]),
        ),
    ] {
        let value = session.eval_probe(source).unwrap();
        session.collect().unwrap();
        assert_eq!(
            decoder.decode_session(&mut session, &value).unwrap(),
            expected,
            "{source}"
        );
    }
}

#[test]
fn collection_decoder_rejects_nominal_impostors_malformed_counts_and_missing_backing() {
    let mut session = Session::new_repl().unwrap();
    let mut decoder = Decoder::capture(&mut session, 100_000).unwrap();
    for source in [
        "(deftype PersistentVector [meta cnt shift root tail hash]) (new PersistentVector nil 0 5 nil nil nil)",
        "(new suss.core/PersistentVector nil -1 5 nil nil nil)",
        "(new suss.core/PersistentVector nil 1 5 nil nil nil)",
        "(new suss.core/PersistentVector nil 33 5 nil nil nil)",
        "(new suss.core/PersistentVector nil 0 5 (new suss.core/VectorNode 42 (suss.core/make-array 32)) (suss.core/array) nil)",
        "(new suss.core/List nil 1 nil 2 nil)",
        "(new suss.core/List nil 1 nil -1 nil)",
        "(new suss.core/Cons nil 1 42 nil)",
    ] {
        let value = session.eval_probe(source).unwrap();
        session.collect().unwrap();
        assert!(
            decoder.decode_session(&mut session, &value).is_err(),
            "{source}"
        );
    }
}

#[test]
fn indexed_sequences_and_map_entries_decode_actual_storage_after_gc() {
    let mut session = Session::new_repl().unwrap();
    let mut decoder = Decoder::capture(&mut session, 4096).unwrap();
    for (source, expected) in [
        (
            "(new suss.core/IndexedSeq (array 1 nil false) 1 nil)",
            Observation::List(vec![Observation::Nil, Observation::Bool(false)]),
        ),
        (
            r#"(new suss.core/IndexedSeq "A😀\uD800" 1 nil)"#,
            Observation::List(vec![
                Observation::String(vec![0xd83d]),
                Observation::String(vec![0xde00]),
                Observation::String(vec![0xd800]),
            ]),
        ),
        (
            "(new suss.core/MapEntry :ready [42] nil)",
            Observation::Vector(vec![
                Observation::Keyword(None, "ready".encode_utf16().collect()),
                Observation::Vector(vec![Observation::Long(42)]),
            ]),
        ),
        (
            "(new suss.core/Cons nil 0 (new suss.core/IndexedSeq (array 1 2) 0 nil) nil)",
            Observation::List(
                vec![0_i64, 1, 2]
                    .into_iter()
                    .map(Observation::Long)
                    .collect(),
            ),
        ),
    ] {
        let value = session.eval_probe(source).unwrap();
        session.collect().unwrap();
        assert_eq!(
            decoder.decode_session(&mut session, &value).unwrap(),
            expected,
            "{source}"
        );
    }
}

#[test]
fn indexed_sequences_reject_invalid_offsets_and_storage() {
    let mut session = Session::new_repl().unwrap();
    let mut decoder = Decoder::capture(&mut session, 4096).unwrap();
    for source in [
        "(new suss.core/IndexedSeq (array 1 2) -1 nil)",
        "(new suss.core/IndexedSeq (array 1 2) 0.5 nil)",
        "(new suss.core/IndexedSeq (array 1 2) 3 nil)",
        "(new suss.core/IndexedSeq nil 0 nil)",
        "(new suss.core/IndexedSeq [] 0 nil)",
    ] {
        let value = session.eval_probe(source).unwrap();
        session.collect().unwrap();
        assert!(
            decoder.decode_session(&mut session, &value).is_err(),
            "{source}"
        );
    }
}

fn unordered_exact<T: PartialEq + std::fmt::Debug>(mut actual: Vec<T>, expected: Vec<T>) {
    assert_eq!(actual.len(), expected.len());
    for value in expected {
        let position = actual
            .iter()
            .position(|item| item == &value)
            .unwrap_or_else(|| panic!("missing {value:?} in {actual:?}"));
        actual.remove(position);
    }
    assert!(actual.is_empty());
}

#[test]
fn map_decoder_observes_array_and_hash_storage_nil_keys_and_large_tries() {
    let mut session = Session::new_repl().unwrap();
    // Bound the intentionally large fixture construction separately from the
    // decoder traversal budget; default interactive fuel is only10million.
    session.set_operation_fuel(1_000_000_000);
    let mut decoder = Decoder::capture(&mut session, 100_000).unwrap();
    for source in ["{}", "(hash-map)"] {
        let value = session.eval_probe(source).unwrap();
        session.collect().unwrap();
        assert_eq!(
            decoder.decode_session(&mut session, &value).unwrap(),
            Observation::Map(vec![])
        );
    }
    let value = session.eval_probe("{nil [42] :ready false}").unwrap();
    session.collect().unwrap();
    let Observation::Map(pairs) = decoder.decode_session(&mut session, &value).unwrap() else {
        panic!("expected map")
    };
    unordered_exact(
        pairs,
        vec![
            (
                Observation::Nil,
                Observation::Vector(vec![Observation::Long(42)]),
            ),
            (
                Observation::Keyword(None, "ready".encode_utf16().collect()),
                Observation::Bool(false),
            ),
        ],
    );
    for count in [9, 33, 80] {
        let value = session.eval_probe(&format!("(loop [i 0 xs {{}}] (if (= i {count}) (assoc xs nil true) (recur (inc i) (assoc xs i (- 0 i)))))")).unwrap();
        session.collect().unwrap();
        let Observation::Map(pairs) = decoder.decode_session(&mut session, &value).unwrap() else {
            panic!("expected map")
        };
        let mut expected = (0..count)
            .map(|i| {
                (
                    Observation::Long(i as i64),
                    Observation::Long(-(i as i64)),
                )
            })
            .collect::<Vec<_>>();
        expected.push((Observation::Nil, Observation::Bool(true)));
        unordered_exact(pairs, expected);
    }
    let value = session.eval_probe(r#"(hash-map "Aa" 1 "BB" 2)"#).unwrap();
    session.collect().unwrap();
    let Observation::Map(pairs) = decoder.decode_session(&mut session, &value).unwrap() else {
        panic!("expected map")
    };
    unordered_exact(
        pairs,
        vec![
            (
                Observation::String("Aa".encode_utf16().collect()),
                Observation::Long(1),
            ),
            (
                Observation::String("BB".encode_utf16().collect()),
                Observation::Long(2),
            ),
        ],
    );
}

#[test]
fn set_decoder_preserves_nested_elements_and_nil_through_gc() {
    let mut session = Session::new_repl().unwrap();
    // Bound the intentionally large fixture construction separately from the
    // decoder traversal budget; default interactive fuel is only10million.
    session.set_operation_fuel(1_000_000_000);
    let mut decoder = Decoder::capture(&mut session, 100_000).unwrap();
    for count in [0, 1, 33, 80] {
        let value = session.eval_probe(&format!("(loop [i 0 xs #{{}}] (if (= i {count}) (conj xs nil [42]) (recur (inc i) (conj xs i))))")).unwrap();
        session.collect().unwrap();
        let Observation::Set(elements) = decoder.decode_session(&mut session, &value).unwrap()
        else {
            panic!("expected set")
        };
        let mut expected = (0..count)
            .map(|i| Observation::Long(i as i64))
            .collect::<Vec<_>>();
        expected.extend([
            Observation::Nil,
            Observation::Vector(vec![Observation::Long(42)]),
        ]);
        unordered_exact(elements, expected);
    }
}

#[test]
fn map_set_decoder_rejects_invalid_counts_flags_and_node_storage() {
    let mut session = Session::new_repl().unwrap();
    let mut decoder = Decoder::capture(&mut session, 100_000).unwrap();
    for source in [
        "(new suss.core/PersistentArrayMap nil 1 (array) nil)",
        "(new suss.core/PersistentHashMap nil 1 nil false nil nil)",
        "(new suss.core/PersistentHashMap nil 0 nil 42 nil nil)",
        "(new suss.core/PersistentHashMap nil 1 (new suss.core/BitmapIndexedNode nil 1 (array)) false nil nil)",
        "(new suss.core/PersistentHashMap nil 1 (new suss.core/ArrayNode nil 1 (make-array 31)) false nil nil)",
        "(new suss.core/PersistentHashMap nil 1 (new suss.core/HashCollisionNode nil 42 1 (array)) false nil nil)",
        "(new suss.core/PersistentHashSet nil [1] nil)",
    ] {
        let value = session.eval_probe(source).unwrap();
        session.collect().unwrap();
        assert!(
            decoder.decode_session(&mut session, &value).is_err(),
            "{source}"
        );
    }
}

#[test]
fn vector_chunk_sequences_observe_rest_and_cons_tails_across_chunks_after_gc() {
    let mut session = Session::new_repl().unwrap();
    session.set_operation_fuel(1_000_000_000);
    let mut decoder = Decoder::capture(&mut session, 100_000).unwrap();
    for count in [1, 3, 31, 32, 33, 65, 80] {
        for prepend in [false, true] {
            let vector =
                format!("(loop [i 0 v []] (if (= i {count}) v (recur (inc i) (conj v i))))");
            let source = if prepend {
                format!("(cons -1 (rest {vector}))")
            } else {
                format!("(rest {vector})")
            };
            let value = session.eval_probe(&source).unwrap();
            session.collect().unwrap();
            let mut expected = Vec::new();
            if prepend {
                expected.push(Observation::Long(-1));
            }
            expected.extend((1..count).map(|i| Observation::Long(i as i64)));
            assert_eq!(
                decoder.decode_session(&mut session, &value).unwrap(),
                Observation::List(expected),
                "{source}"
            );
        }
    }
    let retained = session.eval_probe("(rest [0 1 2])").unwrap();
    session.eval_probe("(ns suss.core) (def ChunkedSeq nil)").unwrap();
    session.collect().unwrap();
    assert_eq!(
        decoder.decode_session(&mut session, &retained).unwrap(),
        Observation::List(vec![
            Observation::Long(1),
            Observation::Long(2)
        ])
    );
}

#[test]
fn chunk_sequences_reject_malformed_storage_and_ignore_unobserved_prefix_values() {
    let mut session = Session::new_repl().unwrap();
    let mut decoder = Decoder::capture(&mut session, 4096).unwrap();
    for source in [
        "(new suss.core/ChunkedSeq [1 2] (array 1 2) -1 0 nil nil)",
        "(new suss.core/ChunkedSeq [1 2] (array 1 2) 0 2 nil nil)",
        "(new suss.core/ChunkedSeq [1 2] (array 1) 0 0 nil nil)",
        "(new suss.core/ChunkedSeq '(1 2) (array 1 2) 0 0 nil nil)",
        "(new suss.core/ChunkedSeq [1 2] nil 0 0 nil nil)",
    ] {
        let value = session.eval_probe(source).unwrap();
        session.collect().unwrap();
        assert!(
            decoder.decode_session(&mut session, &value).is_err(),
            "{source}"
        );
    }
    // The raw chunk's first value and backing vector's first chunk are not
    // observed by this cursor. Decoding those unrelated values would reject
    // a perfectly observable suffix containing ordinary scalar values.
    let value = session.eval_probe("(let [opaque (fn [] 42) v (loop [i 0 v []] (if (= i 33) v (recur (inc i) (conj v (if (< i 32) opaque 7)))))] (new suss.core/ChunkedSeq v (array opaque 7) 32 1 nil nil))");
    // This deliberately inconsistent final chunk must still fail its bounds.
    let value = value.unwrap();
    assert!(decoder.decode_session(&mut session, &value).is_err());
    let value = session.eval_probe("(let [opaque (fn [] 42) v (loop [i 0 v []] (if (= i 34) v (recur (inc i) (conj v (if (< i 32) opaque 7)))))] (new suss.core/ChunkedSeq v (array opaque 7) 32 1 nil nil))").unwrap();
    session.collect().unwrap();
    assert_eq!(
        decoder.decode_session(&mut session, &value).unwrap(),
        Observation::List(vec![Observation::Long(7)])
    );
}

#[test]
fn vector_decoder_rejects_count_exceeding_trie_capacity() {
    let mut session = Session::new_repl().unwrap();
    session.set_operation_fuel(1_000_000_000);
    let mut decoder = Decoder::capture(&mut session, 100_000).unwrap();
    // A genuine 1025-element vector has a shift-5 root containing 1024
    // entries. Raising count without growing that root wraps branch selection
    // at index1024 and must not turn malformed storage into observed values.
    // Forging malformed storage needs core internals; the probe is privileged.
    let value = session.eval_probe("(let [v (loop [i 0 v []] (if (= i 1025) v (recur (inc i) (conj v i))))] (new suss.core/PersistentVector nil 1057 5 (.-root v) (array 7) nil))").unwrap();
    session.collect().unwrap();
    let Err(error) = decoder.decode_session(&mut session, &value) else {
        panic!("decoder accepted a vector count exceeding its trie capacity");
    };
    assert!(
        error
            .to_string()
            .contains("Vector count exceeds trie capacity"),
        "{error}"
    );
}

#[test]
fn scalar_decoder_rejects_mutable_number_impostor() {
    let mut session = Session::new_repl().unwrap();
    let anchor = session.eval_probe("42.0").unwrap();
    session
        .inspect(&anchor, |mut store, canonical| {
            let canonical_type = canonical
                .unwrap_anyref()
                .unwrap()
                .as_struct(&store)?
                .unwrap()
                .ty(&store)?;
            let ty = wasmtime::StructType::new(
                store.engine(),
                [wasmtime::FieldType::new(
                    wasmtime::Mutability::Var,
                    wasmtime::StorageType::ValType(wasmtime::ValType::F64),
                )],
            )?;
            assert!(
                !wasmtime::StructType::eq(&canonical_type, &ty),
                "the mutable impostor must be a different Wasm type"
            );
            let allocator = wasmtime::StructRefPre::new(&mut store, ty);
            let object =
                wasmtime::StructRef::new(&mut store, &allocator, &[Val::F64(42.0_f64.to_bits())])?;
            let value = Val::AnyRef(Some(object.to_anyref()));
            assert!(
                Decoder::new(16).decode(&mut store, &value).is_err(),
                "a mutable unary f64 structure is not an ABI2 boxed number"
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn string_decoder_rejects_immutable_utf16_array_impostor() {
    let mut session = Session::new_repl().unwrap();
    let anchor = session.eval_probe("\"A\"").unwrap();
    session
        .inspect(&anchor, |mut store, canonical| {
            let canonical_type = canonical
                .unwrap_anyref()
                .unwrap()
                .as_array(&store)?
                .unwrap()
                .ty(&store)?;
            let ty = wasmtime::ArrayType::new(
                store.engine(),
                wasmtime::FieldType::new(wasmtime::Mutability::Const, wasmtime::StorageType::I16),
            );
            assert!(
                !wasmtime::ArrayType::eq(&canonical_type, &ty),
                "the immutable impostor must be a different Wasm type"
            );
            let allocator = wasmtime::ArrayRefPre::new(&mut store, ty);
            let object = wasmtime::ArrayRef::new(&mut store, &allocator, &Val::I32(65), 1)?;
            let value = Val::AnyRef(Some(object.to_anyref()));
            assert!(
                Decoder::new(16).decode(&mut store, &value).is_err(),
                "an immutable i16 array is not an ABI2 language string"
            );
            Ok(())
        })
        .unwrap();
}



#[test]
fn bigint_decoder_reads_exact_storage_after_gc_without_guest_printers() {
    let mut session = Session::new_repl().unwrap();
    let mut decoder = Decoder::capture(&mut session, 10000).unwrap();
    for (source, expected) in [
        ("0N", "0"), ("-1N", "-1"),
        ("18446744073709551617N", "18446744073709551617"),
        ("18446744055262807542290448384N", "18446744055262807542290448384"),
        ("-340282366920938463463374607431768211457N", "-340282366920938463463374607431768211457"),
        ("(biginteger 18446744073709551617N)", "18446744073709551617"),
    ] {
        let value = session.eval_probe(source).unwrap();
        session.collect().unwrap();
        decoder.refill(10000);
        assert_eq!(decoder.decode_session(&mut session, &value).unwrap(), Observation::BigInt(expected.into()), "{source}");
    }
}


#[test]
fn ratio_decoder_reads_actual_exact_children_after_gc() {
    let mut session = Session::new_repl().unwrap();
    let mut decoder = Decoder::capture(&mut session, 10000).unwrap();
    for (source, numerator, denominator) in [
        ("1/3", "1", "3"), ("-2/6", "-1", "3"),
        ("18446744073709551617/2", "18446744073709551617", "2"),
        ("(let [x 1/3] (+ x x))", "2", "3"),
    ] {
        let value = session.eval_probe(source).unwrap();
        session.collect().unwrap(); decoder.refill(10000);
        assert_eq!(decoder.decode_session(&mut session,&value).unwrap(), Observation::Ratio(numerator.into(),denominator.into()), "{source}");
    }
}

#[test]
fn decimal_decoder_rejects_mutable_owners_and_malformed_exact_coefficients() {
    let mut session = Session::new_repl().unwrap();
    let value = session.eval_probe("1.50M").unwrap();
    let mut decoder = Decoder::capture_numeric(&mut session, 1000).unwrap();
    session.collect().unwrap();
    session.inspect(&value, |mut store, value| {
        let owner = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
        let canonical = owner.ty(&store)?;
        let fields = owner.fields(&mut store)?.collect::<Vec<_>>();
        assert_eq!(decoder.decode(&mut store, &value)?,
            Observation::Decimal { unscaled: "150".into(), scale: 2 });
        for mutable in 0..2 {
            let types = canonical.fields().enumerate().map(|(index, field)|
                wasmtime::FieldType::new(if index == mutable { wasmtime::Mutability::Var }
                    else { field.mutability() }, field.element_type().clone()));
            let ty = wasmtime::StructType::new(store.engine(), types)?;
            assert!(!wasmtime::StructType::eq(&canonical, &ty));
            let pre = wasmtime::StructRefPre::new(&mut store, ty);
            let forged = wasmtime::StructRef::new(&mut store, &pre, &fields)?;
            decoder.refill(1000);
            assert!(decoder.decode(&mut store, &Val::AnyRef(Some(forged.to_anyref()))).is_err());
        }
        let coefficient = fields[0].unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
        let original = coefficient.fields(&mut store)?.collect::<Vec<_>>();
        let integer_ty = coefficient.ty(&store)?;
        let integer_pre = wasmtime::StructRefPre::new(&mut store, integer_ty);
        let decimal_pre = wasmtime::StructRefPre::new(&mut store, canonical);
        for (index, invalid) in [(0, 0), (2, i32::MAX), (3, 2)] {
            let mut broken = original.clone();
            broken[index] = Val::I32(invalid);
            let integer = wasmtime::StructRef::new(&mut store, &integer_pre, &broken)?;
            let forged = wasmtime::StructRef::new(&mut store, &decimal_pre,
                &[Val::AnyRef(Some(integer.to_anyref())), Val::I32(2)])?;
            decoder.refill(1000);
            assert!(decoder.decode(&mut store, &Val::AnyRef(Some(forged.to_anyref()))).is_err());
        }
        decoder.refill(1);
        assert!(decoder.decode(&mut store, &value).is_err());
        Ok(())
    }).unwrap();
}

#[test]
fn sorted_collection_decoder_preserves_exact_fields_after_gc_without_guest_comparison() {
    let dec = |unscaled: &str, scale| Observation::Decimal {
        unscaled: unscaled.into(),
        scale,
    };
    let mut cases = vec![
        ("(sorted-map)".to_owned(), Observation::Map(vec![])),
        (
            "(sorted-map 2 2.00M 1 1.0M)".to_owned(),
            Observation::Map(vec![
                (Observation::Long(1), dec("10", 1)),
                (Observation::Long(2), dec("200", 2)),
            ]),
        ),
        (
            "(sorted-map-by > 2 2.00M 1 1.0M)".to_owned(),
            Observation::Map(vec![
                (Observation::Long(2), dec("200", 2)),
                (Observation::Long(1), dec("10", 1)),
            ]),
        ),
        ("(sorted-set)".to_owned(), Observation::Set(vec![])),
        (
            "(sorted-set 2M 1.00M)".to_owned(),
            Observation::Set(vec![dec("100", 2), dec("2", 0)]),
        ),
    ];
    cases.push((
        "(into (sorted-map) (map (fn [x] [x (bigdec x)]) (range 48)))".into(),
        Observation::Map(
            (0..48)
                .map(|x| (Observation::Long(x), dec(&x.to_string(), 0)))
                .collect(),
        ),
    ));
    let mut failures = Vec::new();
    for phase in ["Runtime", "Macro"] {
        let mut session = if phase == "Runtime" {
            Session::new_repl().unwrap()
        } else {
            Session::new_macro().unwrap()
        };
        let mut decoder = Decoder::capture(&mut session, 10_000).unwrap();
        let values = cases
            .iter()
            .map(|(source, _)| session.eval_probe(source).unwrap())
            .collect::<Vec<_>>();
        session.collect().unwrap();
        // Decoding reads only retained canonical storage. Neither printer nor
        // guest equality can turn malformed/unknown storage into a host match.
        session.eval_probe("(ns suss.core) (def pr-str (fn [& xs] (throw (ex-info \"guest printer called\" {})))) (def = (fn [& xs] (throw (ex-info \"guest comparison called\" {}))))").unwrap();
        for ((source, expected), value) in cases.iter().zip(&values) {
            decoder.refill(10_000);
            let observed = decoder.decode_session(&mut session, value);
            match observed {
                Ok(actual) if actual == *expected => (),
                actual => failures.push(format!(
                    "{phase} {source}: {actual:?}; expected {expected:?}"
                )),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn sorted_collection_decoder_rejects_count_layout_and_budget_errors() {
    let mut session = Session::new_repl().unwrap();
    let mut decoder = Decoder::capture(&mut session, 10_000).unwrap();
    for (source, expected) in [
        (
            "(new suss.core/PersistentTreeMap compare nil 1 nil nil)",
            "count disagrees",
        ),
        (
            "(new suss.core/PersistentTreeMap compare (new suss.core/BlackNode 1 2 nil nil nil) 0 nil nil)",
            "count disagrees",
        ),
        (
            "(new suss.core/PersistentTreeMap compare [1] 1 nil nil)",
            "canonical tree node",
        ),
        (
            "(new suss.core/PersistentTreeSet nil (hash-map) nil)",
            "canonical backing tree map",
        ),
        (
            "(deftype DecoderTreeMapLookalike [comp tree cnt meta cached]) (new DecoderTreeMapLookalike compare nil 0 nil nil)",
            "Unsupported ABI2 value layout",
        ),
    ] {
        let value = session.eval_probe(source).unwrap();
        session.collect().unwrap();
        decoder.refill(10_000);
        let error = decoder.decode_session(&mut session, &value).unwrap_err();
        assert!(error.to_string().contains(expected), "{source}: {error}");
    }
    let value = session.eval_probe("(sorted-map 1 1M 2 2M)").unwrap();
    decoder.refill(2);
    let error = decoder.decode_session(&mut session, &value).unwrap_err();
    assert!(error.to_string().contains("traversal limit"), "{error}");
}

#[test]
fn case_hash_preserves_retained_fields_after_gc_and_public_rebinding() {
    let decimal = || Observation::Decimal { unscaled: "100".into(), scale: 2 };
    let key = || Observation::Keyword(None, "a".encode_utf16().collect());
    let cases = vec![
        ("1.00M", decimal()),
        ("-1", Observation::Long(-1)),
        ("(biginteger -1)", Observation::BigInt("-1".into())),
        ("2/3", Observation::Ratio("2".into(), "3".into())),
        ("-0.0", Observation::Number((-0.0_f64).to_bits())),
        ("[1.00M]", Observation::Vector(vec![decimal()])),
        ("{:a 1.00M}", Observation::Map(vec![(key(), decimal())])),
        ("#{1.00M}", Observation::Set(vec![decimal()])),
        ("'app/name", Observation::Symbol(Some("app".encode_utf16().collect()), "name".encode_utf16().collect())),
        (":Aa", Observation::Keyword(None, "Aa".encode_utf16().collect())),
        ("\"Aa\"", Observation::String("Aa".encode_utf16().collect())),
    ];
    let primary = [
        include_str!("../../../results/m12-case-dispatch/numeric-hash-cases.json"),
        include_str!("../../../results/m12-case-dispatch/composite-hash-cases.json"),
    ].iter().flat_map(|s| serde_json::from_str::<Vec<(String, String)>>(s).unwrap())
        .collect::<std::collections::BTreeMap<_, _>>();
    for phase in ["Runtime", "Macro"] {
        let mut session = if phase == "Runtime" { Session::new_repl().unwrap() } else { Session::new_macro().unwrap() };
        let mut decoder = Decoder::capture(&mut session, 10_000).unwrap();
        let functions = cases.iter().map(|(source, _)| session.eval_probe(&format!(
            "(let [x {source}] (fn [] [(suss.bootstrap/case-hash-code x) x (suss.bootstrap/case-equiv x {source})]))"
        )).unwrap()).collect::<Vec<_>>();
        session.collect().unwrap();
        session.eval_probe("(ns suss.core) (def pr-str (fn [& xs] (throw (ex-info \"guest printer called\" {})))) (def = (fn [& xs] (throw (ex-info \"public equality called\" {})))) (def hash (fn [& xs] (throw (ex-info \"public hash called\" {})))) (def symbol? (fn [x] false)) (def keyword? (fn [x] false)) (def number? (fn [x] false)) (def sequential? (fn [x] false)) (def map? (fn [x] false)) (def set? (fn [x] false))").unwrap();
        for ((source, expected), function) in cases.iter().zip(functions) {
            let hash = primary.get(&format!("(suss.bootstrap/case-hash-code {source})"))
                .or_else(|| primary.get(&format!("(suss.bootstrap/case-number-hash {source})")))
                .unwrap().parse::<i64>().unwrap();
            let value = session.invoke(&function, &[]).unwrap();
            session.collect().unwrap();
            decoder.refill(10_000);
            let observed = decoder.decode_session(&mut session, &value).unwrap();
            let Observation::Vector(fields) = observed else { panic!("{phase}: {source}: {observed:?}"); };
            assert_eq!(fields.len(), 3, "{phase}: {source}");
            assert_eq!(fields[0], Observation::Long(hash), "{phase}: {source}: hash");
            assert_eq!(fields[1], *expected, "{phase}: {source}: retained fields");
            assert_eq!(fields[2], Observation::Bool(true), "{phase}: {source}: fixed core equality");
        }
    }
}

#[test]
fn compiled_case_retains_literal_values_after_gc_and_keeps_collision_equality_live() {
    use suss_compile::portable_macros::CompiledMacros;
    let decimal = || Observation::Decimal { unscaled: "100".into(), scale: 2 };
    let symbol = |s: &str| Observation::Symbol(None, s.encode_utf16().collect());
    let cases = vec![
        ("(let [x 1.00M] (fn [] (case :hit :hit x nil)))", decimal()),
        ("(let [x [1.00M]] (fn [] (case x [1.00M] x nil)))", Observation::Vector(vec![decimal()])),
        ("(let [x [1.00M]] (fn [] (case x [1.0M] x nil)))", Observation::Nil),
        ("(let [x {:a 1.00M}] (fn [] (case x {:a 1.00M} x nil)))", Observation::Map(vec![(Observation::Keyword(None, vec![97]), decimal())])),
        ("(let [x #{1.00M}] (fn [] (case x #{1.00M} x nil)))", Observation::Set(vec![decimal()])),
        ("(let [x [1N]] (fn [] (case x [1] x nil)))", Observation::Vector(vec![Observation::BigInt("1".into())])),
        ("(let [x '(a b)] (fn [] (case x [a b] x nil)))", Observation::List(vec![symbol("a"), symbol("b")])),
        (r#"(fn [] (try (case "BB" "Aa" :aa "BB" :bb :miss) (catch :default e (ex-message e))))"#,
            Observation::String("public equality called".encode_utf16().collect())),
    ];
    for phase in ["Runtime", "Macro"] {
        let mut session = if phase == "Runtime" { Session::new_repl().unwrap() } else { Session::new_macro().unwrap() };
        session.set_operation_fuel(100_000_000);
        let mut macros = CompiledMacros::new().unwrap();
        let mut decoder = Decoder::capture(&mut session, 10_000).unwrap();
        let functions = cases.iter().map(|(source, _)| session.eval_with_macros(source, &mut macros).unwrap()).collect::<Vec<_>>();
        session.collect().unwrap();
        session.eval_probe("(ns suss.core) (def pr-str (fn [& xs] (throw (ex-info \"guest printer called\" {})))) (def = (fn [& xs] (throw (ex-info \"public equality called\" {})))) (def hash (fn [& xs] (throw (ex-info \"public hash called\" {})))) (def keyword? (fn [x] false))").unwrap();
        for ((source, expected), function) in cases.iter().zip(functions) {
            let value = session.invoke(&function, &[]).unwrap();
            session.collect().unwrap();
            decoder.refill(10_000);
            assert_eq!(decoder.decode_session(&mut session, &value).unwrap(), *expected, "{phase}: {source}");
        }
    }
}
