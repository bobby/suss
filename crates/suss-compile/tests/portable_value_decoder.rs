//! Executing foundations for migrating the complete expression corpus to ABI2.
#![cfg(not(target_family = "wasm"))]
#[path = "support/portable_decode.rs"]
mod portable_decode;
use portable_decode::{Decoder, Observation};
use suss_compile::portable_session::Session;
use wasmtime::{AnyRef, I31, Val};

#[test]
fn scalar_decoder_preserves_bits_and_language_sentinels_after_gc() {
    let mut session = Session::new_repl().unwrap();
    for (source, expected) in [
        ("nil", Observation::Nil),
        ("false", Observation::Bool(false)),
        ("true", Observation::Bool(true)),
        ("42", Observation::Number(42.0_f64.to_bits())),
        ("-0.0", Observation::Number((-0.0_f64).to_bits())),
        ("##NaN", Observation::Number(f64::NAN.to_bits())),
        ("##Inf", Observation::Number(f64::INFINITY.to_bits())),
        ("##-Inf", Observation::Number(f64::NEG_INFINITY.to_bits())),
    ] {
        let value = session.eval(source).unwrap();
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
    let value = session.eval(r#""A😀\uD800x\uDC00λ""#).unwrap();
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
    let value = session.eval("nil").unwrap();
    session
        .inspect(&value, |mut store, _| {
            for tag in [1, 6, 99] {
                let forged = Val::AnyRef(Some(AnyRef::from_i31(
                    &mut store,
                    I31::new_u32(tag).unwrap(),
                )));
                assert!(
                    Decoder::new(16)
                        .decode(&mut store, &forged)
                        .unwrap_err()
                        .to_string()
                        .contains("sentinel")
                );
            }
            for malformed in [
                Val::null_any_ref(),
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
        let value = session.eval(source).unwrap();
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
    let value = session.eval(r#""abcdef""#).unwrap();
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
        let value = session.eval(source).unwrap();
        session.collect().unwrap();
        assert_eq!(
            decoder.decode_session(&mut session, &value).unwrap(),
            expected
        );
    }
    let retained = session.eval(":ready").unwrap();
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
    session.eval("(ns suss.core) (def Keyword nil)").unwrap();
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
    let old = session.eval(":ready").unwrap();
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
        let value = session.eval(source).unwrap();
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
            .eval(&format!(
                "(loop [i 0 xs []] (if (= i {count}) xs (recur (inc i) (conj xs i))))"
            ))
            .unwrap();
        session.collect().unwrap();
        assert_eq!(
            decoder.decode_session(&mut session, &value).unwrap(),
            Observation::Vector(
                (0..count)
                    .map(|i| Observation::Number((i as f64).to_bits()))
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
            Observation::Vector(vec![Observation::Number(42.0_f64.to_bits())]),
        ])
    );
    let retained = session.eval("[42]").unwrap();
    session
        .eval("(ns suss.core) (def PersistentVector nil) (def VectorNode nil)")
        .unwrap();
    session.collect().unwrap();
    assert_eq!(
        decoder.decode_session(&mut session, &retained).unwrap(),
        Observation::Vector(vec![Observation::Number(42.0_f64.to_bits())])
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
                vec![1.0_f64, 2.0, 3.0]
                    .into_iter()
                    .map(|n| Observation::Number(n.to_bits()))
                    .collect(),
            ),
        ),
        (
            "(cons 1 (list 2 3))",
            Observation::List(
                vec![1.0_f64, 2.0, 3.0]
                    .into_iter()
                    .map(|n| Observation::Number(n.to_bits()))
                    .collect(),
            ),
        ),
        (
            "(new suss.core/Cons nil 1 [2 3] nil)",
            Observation::List(
                vec![1.0_f64, 2.0, 3.0]
                    .into_iter()
                    .map(|n| Observation::Number(n.to_bits()))
                    .collect(),
            ),
        ),
        (
            "(list [42] (list nil false))",
            Observation::List(vec![
                Observation::Vector(vec![Observation::Number(42.0_f64.to_bits())]),
                Observation::List(vec![Observation::Nil, Observation::Bool(false)]),
            ]),
        ),
    ] {
        let value = session.eval(source).unwrap();
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
        let value = session.eval(source).unwrap();
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
                Observation::Vector(vec![Observation::Number(42.0_f64.to_bits())]),
            ]),
        ),
        (
            "(new suss.core/Cons nil 0 (new suss.core/IndexedSeq (array 1 2) 0 nil) nil)",
            Observation::List(
                vec![0.0_f64, 1.0, 2.0]
                    .into_iter()
                    .map(|n| Observation::Number(n.to_bits()))
                    .collect(),
            ),
        ),
    ] {
        let value = session.eval(source).unwrap();
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
        let value = session.eval(source).unwrap();
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
        let value = session.eval(source).unwrap();
        session.collect().unwrap();
        assert_eq!(
            decoder.decode_session(&mut session, &value).unwrap(),
            Observation::Map(vec![])
        );
    }
    let value = session.eval("{nil [42] :ready false}").unwrap();
    session.collect().unwrap();
    let Observation::Map(pairs) = decoder.decode_session(&mut session, &value).unwrap() else {
        panic!("expected map")
    };
    unordered_exact(
        pairs,
        vec![
            (
                Observation::Nil,
                Observation::Vector(vec![Observation::Number(42.0_f64.to_bits())]),
            ),
            (
                Observation::Keyword(None, "ready".encode_utf16().collect()),
                Observation::Bool(false),
            ),
        ],
    );
    for count in [9, 33, 80] {
        let value = session.eval(&format!("(loop [i 0 xs {{}}] (if (= i {count}) (assoc xs nil true) (recur (inc i) (assoc xs i (- 0 i)))))")).unwrap();
        session.collect().unwrap();
        let Observation::Map(pairs) = decoder.decode_session(&mut session, &value).unwrap() else {
            panic!("expected map")
        };
        let mut expected = (0..count)
            .map(|i| {
                (
                    Observation::Number((i as f64).to_bits()),
                    Observation::Number((0.0_f64 - i as f64).to_bits()),
                )
            })
            .collect::<Vec<_>>();
        expected.push((Observation::Nil, Observation::Bool(true)));
        unordered_exact(pairs, expected);
    }
    let value = session.eval(r#"(hash-map "Aa" 1 "BB" 2)"#).unwrap();
    session.collect().unwrap();
    let Observation::Map(pairs) = decoder.decode_session(&mut session, &value).unwrap() else {
        panic!("expected map")
    };
    unordered_exact(
        pairs,
        vec![
            (
                Observation::String("Aa".encode_utf16().collect()),
                Observation::Number(1.0_f64.to_bits()),
            ),
            (
                Observation::String("BB".encode_utf16().collect()),
                Observation::Number(2.0_f64.to_bits()),
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
        let value = session.eval(&format!("(loop [i 0 xs #{{}}] (if (= i {count}) (conj xs nil [42]) (recur (inc i) (conj xs i))))")).unwrap();
        session.collect().unwrap();
        let Observation::Set(elements) = decoder.decode_session(&mut session, &value).unwrap()
        else {
            panic!("expected set")
        };
        let mut expected = (0..count)
            .map(|i| Observation::Number((i as f64).to_bits()))
            .collect::<Vec<_>>();
        expected.extend([
            Observation::Nil,
            Observation::Vector(vec![Observation::Number(42.0_f64.to_bits())]),
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
        let value = session.eval(source).unwrap();
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
            let value = session.eval(&source).unwrap();
            session.collect().unwrap();
            let mut expected = Vec::new();
            if prepend {
                expected.push(Observation::Number((-1.0_f64).to_bits()));
            }
            expected.extend((1..count).map(|i| Observation::Number((i as f64).to_bits())));
            assert_eq!(
                decoder.decode_session(&mut session, &value).unwrap(),
                Observation::List(expected),
                "{source}"
            );
        }
    }
    let retained = session.eval("(rest [0 1 2])").unwrap();
    session.eval("(ns suss.core) (def ChunkedSeq nil)").unwrap();
    session.collect().unwrap();
    assert_eq!(
        decoder.decode_session(&mut session, &retained).unwrap(),
        Observation::List(vec![
            Observation::Number(1.0_f64.to_bits()),
            Observation::Number(2.0_f64.to_bits())
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
        let value = session.eval(source).unwrap();
        session.collect().unwrap();
        assert!(
            decoder.decode_session(&mut session, &value).is_err(),
            "{source}"
        );
    }
    // The raw chunk's first value and backing vector's first chunk are not
    // observed by this cursor. Decoding those unrelated values would reject
    // a perfectly observable suffix containing ordinary scalar values.
    let value = session.eval("(let [opaque (fn [] 42) v (loop [i 0 v []] (if (= i 33) v (recur (inc i) (conj v (if (< i 32) opaque 7)))))] (new suss.core/ChunkedSeq v (array opaque 7) 32 1 nil nil))");
    // This deliberately inconsistent final chunk must still fail its bounds.
    let value = value.unwrap();
    assert!(decoder.decode_session(&mut session, &value).is_err());
    let value = session.eval("(let [opaque (fn [] 42) v (loop [i 0 v []] (if (= i 34) v (recur (inc i) (conj v (if (< i 32) opaque 7)))))] (new suss.core/ChunkedSeq v (array opaque 7) 32 1 nil nil))").unwrap();
    session.collect().unwrap();
    assert_eq!(
        decoder.decode_session(&mut session, &value).unwrap(),
        Observation::List(vec![Observation::Number(7.0_f64.to_bits())])
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
    let value = session.eval("(let [v (loop [i 0 v []] (if (= i 1025) v (recur (inc i) (conj v i))))] (new suss.core/PersistentVector nil 1057 5 (.-root v) (array 7) nil))").unwrap();
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
    let anchor = session.eval("42").unwrap();
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
    let anchor = session.eval("\"A\"").unwrap();
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
fn sparse_array_backed_sequences_preserve_present_values_and_reject_observed_holes() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut decoder = Decoder::capture(&mut session, 4096).unwrap();
        for source in [
            "(new suss.core/IndexedSeq (array 7 nil false) 0 nil)",
            "(let [a (array)] (aset a 0 7) (aset a 1 nil) (aset a 2 false) (aset a 4294967295 99) (new suss.core/IndexedSeq a 0 nil))",
        ] {
            let value = session.eval(source).unwrap();
            session.collect().unwrap();
            assert_eq!(
                decoder.decode_session(&mut session, &value).unwrap(),
                Observation::List(vec![
                    Observation::Number(7.0_f64.to_bits()),
                    Observation::Nil,
                    Observation::Bool(false)
                ]),
                "{source}"
            );
        }
        let tail = session
            .eval("(let [a (array)] (aset a 2 7) (new suss.core/IndexedSeq a 2 nil))")
            .unwrap();
        session.collect().unwrap();
        assert_eq!(
            decoder.decode_session(&mut session, &tail).unwrap(),
            Observation::List(vec![Observation::Number(7.0_f64.to_bits())])
        );
        let hole = session
            .eval("(let [a (array)] (aset a 2 7) (new suss.core/IndexedSeq a 1 nil))")
            .unwrap();
        session.collect().unwrap();
        assert!(
            decoder
                .decode_session(&mut session, &hole)
                .unwrap_err()
                .to_string()
                .contains("sentinel")
        );
        let oversized = session
            .eval("(new suss.core/IndexedSeq (js/Array. 4294967295) 0 nil)")
            .unwrap();
        session.collect().unwrap();
        assert!(
            decoder
                .decode_session(&mut session, &oversized)
                .unwrap_err()
                .to_string()
                .contains("source array limit")
        );
    }
}
