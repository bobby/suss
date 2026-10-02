use suss_cli::{portable_macro_data::FormBridge, portable_session::Session};
use suss_compile::portable::hir::Literal;
use suss_reader::forms::Kind;

#[test]
fn native_macro_data_builders_preserve_bits_utf16_canonical_classes_and_shared_roots() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let bridge = FormBridge::new(&mut session).unwrap();
        let same = session
            .eval("(fn [values] (identical? (nth values 0) (nth values 1)))")
            .unwrap();
        let count = session.eval("(fn [value] (count value))").unwrap();
        let before = session.stats();
        let units = vec![0xd800, 0, 0xd83d, 0xde00];
        let text = bridge
            .scalar(&mut session, &Literal::String(units.clone()))
            .unwrap();
        let mut entries = Vec::new();
        for index in 0..17 {
            let key = bridge
                .identifier(&mut session, None, &format!("key{index}"), true, None)
                .unwrap();
            let value = if index == 0 {
                text.clone()
            } else {
                bridge
                    .scalar(&mut session, &Literal::Number(index as f64))
                    .unwrap()
            };
            entries.push((key, value));
        }
        let map = bridge.map_values(&mut session, &entries).unwrap();
        for (index, (key, _)) in entries.iter().enumerate() {
            assert!(
                matches!(bridge.read(&mut session, key, 0..1).unwrap().kind, Kind::Keyword(key) if key.name == format!("key{index}"))
            );
        }
        let size = session.invoke(&count, &[&map]).unwrap();
        assert!(
            matches!(bridge.read(&mut session, &size, 0..1).unwrap().kind, Kind::Number(size) if size == 17.0)
        );
        let shared = bridge
            .vector_values(&mut session, &[map.clone(), map.clone()])
            .unwrap();
        let list = bridge
            .list_values(&mut session, &[text.clone(), map.clone()])
            .unwrap();
        let tag = bridge
            .identifier(&mut session, None, "tag", true, None)
            .unwrap();
        let number = bridge
            .identifier(&mut session, None, "number", false, None)
            .unwrap();
        let metadata = bridge.map_values(&mut session, &[(tag, number)]).unwrap();
        let symbol = bridge
            .identifier(&mut session, Some("sample"), "x", false, Some(&metadata))
            .unwrap();
        let mut scalars = Vec::new();
        for bits in [
            0x8000_0000_0000_0000,
            0x7ff8_0000_0000_0042,
            0x7ff0_0000_0000_0000,
        ] {
            scalars.push((
                bits,
                bridge
                    .scalar(&mut session, &Literal::Number(f64::from_bits(bits)))
                    .unwrap(),
            ));
        }
        session.collect().unwrap();
        assert_eq!(
            session.stats().resident_fragments,
            before.resident_fragments
        );
        assert_eq!(
            session.stats().resident_artifact_bytes,
            before.resident_artifact_bytes
        );
        let result = session.invoke(&same, &[&shared]).unwrap();
        assert_eq!(
            session
                .inspect(&result, |store, value| {
                    Ok(value
                        .unwrap_anyref()
                        .unwrap()
                        .as_i31(&store)?
                        .unwrap()
                        .get_u32())
                })
                .unwrap(),
            4
        );
        let decoded = bridge.read(&mut session, &map, 0..1).unwrap();
        let Kind::Map(values) = decoded.kind else {
            panic!("canonical hash map")
        };
        assert_eq!(values.len(), 34);
        let first = values
            .chunks_exact(2)
            .find(|pair| matches!(&pair[0].kind, Kind::Keyword(key) if key.name == "key0"))
            .unwrap();
        assert!(matches!(&first[1].kind, Kind::String(value) if *value == units));
        assert!(
            matches!(bridge.read(&mut session, &list, 0..1).unwrap().kind, Kind::List(values) if values.len() == 2)
        );
        let decoded = bridge.read(&mut session, &symbol, 0..1).unwrap();
        assert!(
            matches!(&decoded.kind, Kind::Symbol(name) if name.namespace.as_deref() == Some("sample") && name.name == "x")
        );
        assert_eq!(decoded.metadata.len(), 1);
        for (bits, value) in scalars {
            let actual = session
                .inspect(&value, |mut store, value| {
                    Ok(value
                        .unwrap_anyref()
                        .unwrap()
                        .as_struct(&store)?
                        .unwrap()
                        .fields(&mut store)?
                        .next()
                        .unwrap()
                        .unwrap_f64()
                        .to_bits())
                })
                .unwrap();
            assert_eq!(actual, bits);
        }
    }
}

#[test]
fn native_macro_data_builders_enforce_store_identity_and_reset_without_allocating_fragments() {
    let mut owner = Session::new_macro().unwrap();
    let bridge = FormBridge::new(&mut owner).unwrap();
    let value = bridge.scalar(&mut owner, &Literal::Number(42.0)).unwrap();
    let mut foreign = Session::new_macro().unwrap();
    assert!(bridge.scalar(&mut foreign, &Literal::Nil).is_err());
    assert!(
        bridge
            .vector_values(&mut foreign, &[value.clone()])
            .is_err()
    );
    owner.reset().unwrap();
    assert!(bridge.scalar(&mut owner, &Literal::Nil).is_err());
    assert!(bridge.map_values(&mut owner, &[]).is_err());
    let fresh = FormBridge::new(&mut owner).unwrap();
    assert!(fresh.vector_values(&mut owner, &[value]).is_err());
    let nil = fresh.scalar(&mut owner, &Literal::Nil).unwrap();
    assert!(matches!(
        fresh.read(&mut owner, &nil, 0..1).unwrap().kind,
        Kind::Nil
    ));
}

#[test]
fn native_macro_data_maps_use_last_equal_key_and_vectors_cross_tree_boundaries() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        // Building a second-level trie executes the retained core's transient
        // insertion path; give this bounded stress fixture an explicit budget.
        session.set_operation_fuel(100_000_000);
        let bridge = FormBridge::new(&mut session).unwrap();
        let lookup = session.eval("(fn [m k] (get m k))").unwrap();
        let before = session.stats();
        for length in [2, 8, 9, 17] {
            let mut entries = Vec::new();
            for index in 0..length {
                // Distinct objects representing the same key must coalesce.
                let key = bridge
                    .identifier(&mut session, None, "same", true, None)
                    .unwrap();
                let value = bridge
                    .scalar(&mut session, &Literal::Number(index as f64))
                    .unwrap();
                entries.push((key, value));
            }
            let map = bridge.map_values(&mut session, &entries).unwrap();
            session.collect().unwrap();
            assert!(
                matches!(bridge.read(&mut session, &map, 0..1).unwrap().kind, Kind::Map(values) if values.len() == 2)
            );
            let key = bridge
                .identifier(&mut session, None, "same", true, None)
                .unwrap();
            let value = session.invoke(&lookup, &[&map, &key]).unwrap();
            assert!(
                matches!(bridge.read(&mut session, &value, 0..1).unwrap().kind, Kind::Number(value) if value == (length - 1) as f64)
            );
        }
        for length in [0, 1, 31, 32, 33, 1056, 1057] {
            let items = (0..length)
                .map(|index| {
                    bridge
                        .scalar(&mut session, &Literal::Number(index as f64))
                        .unwrap()
                })
                .collect::<Vec<_>>();
            let vector = bridge.vector_values(&mut session, &items).unwrap();
            session.collect().unwrap();
            let Kind::Vector(values) = bridge.read(&mut session, &vector, 0..1).unwrap().kind
            else {
                panic!("canonical vector")
            };
            assert_eq!(values.len(), length);
            for (index, value) in values.iter().enumerate() {
                assert!(matches!(value.kind, Kind::Number(number) if number == index as f64));
            }
        }
        assert_eq!(
            session.stats().resident_fragments,
            before.resident_fragments
        );
        assert_eq!(
            session.stats().resident_artifact_bytes,
            before.resident_artifact_bytes
        );
    }
}

#[test]
fn native_macro_form_transport_is_bounded_data_and_adds_no_resident_code() {
    use suss_reader::forms::{Form, read_forms};
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let bridge = FormBridge::new(&mut session).unwrap();
        let effects = session
            .eval("(def transport-effects 0) (fn [] transport-effects)")
            .unwrap();
        let before = session.stats();
        let input = read_forms("^{:answer 7} ^{:answer (set! transport-effects 99) :doc \"data\"} (unresolved ^number x [1 2] {:a 3 :a 4})").unwrap().remove(0);
        for _ in 0..8 {
            let value = bridge.quote(&mut session, input.clone()).unwrap();
            session.collect().unwrap();
            let form = bridge.read(&mut session, &value, 100..120).unwrap();
            let Kind::List(items) = form.kind else {
                panic!("source data")
            };
            assert!(matches!(&items[0].kind, Kind::Symbol(name) if name.name == "unresolved"));
            assert_eq!(items[1].metadata.len(), 1);
            assert!(
                matches!(&items[3].kind, Kind::Map(entries) if entries.len() == 2 && matches!(entries[1].kind, Kind::Number(4.0)))
            );
            let Kind::Map(metadata) = &form.metadata[0].kind else {
                panic!("reader metadata")
            };
            assert!(matches!(metadata[1].kind, Kind::Number(7.0)));
        }
        let value = session.invoke(&effects, &[]).unwrap();
        assert!(matches!(
            bridge.read(&mut session, &value, 0..1).unwrap().kind,
            Kind::Number(0.0)
        ));
        drop(value);
        let form = |kind| Form {
            span: 0..1,
            metadata: vec![],
            kind,
        };
        let mut deep = form(Kind::Nil);
        for _ in 0..64 {
            deep = form(Kind::List(vec![deep]));
        }
        assert!(
            bridge
                .quote(&mut session, deep)
                .unwrap_err()
                .to_string()
                .contains("64")
        );
        let wide = form(Kind::Vector(vec![form(Kind::Nil); 4096]));
        assert!(
            bridge
                .quote(&mut session, wide)
                .unwrap_err()
                .to_string()
                .contains("4096")
        );
        let large = form(Kind::String(vec![65; 1_048_577]));
        assert!(
            bridge
                .quote(&mut session, large)
                .unwrap_err()
                .to_string()
                .contains("UTF-16")
        );
        let unsupported = form(Kind::Set(vec![]));
        assert!(
            bridge
                .quote(&mut session, unsupported)
                .unwrap_err()
                .to_string()
                .contains("persistent set")
        );
        session.collect().unwrap();
        assert_eq!(
            session.stats().resident_fragments,
            before.resident_fragments
        );
        assert_eq!(
            session.stats().resident_artifact_bytes,
            before.resident_artifact_bytes
        );
        assert_eq!(
            session.stats().external_value_handles,
            before.external_value_handles
        );
    }
}
