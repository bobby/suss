use suss_cli::portable_session::Session;

#[test]
fn compiled_macro_vectors_use_retained_nominal_type_and_persistent_tail() {
    let mut session = Session::new_macro().unwrap();
    session.eval("(def original [10 20 30])").unwrap();
    session.eval("(def extended (-conj original 40))").unwrap();
    session
        .eval("(def changed (-assoc-n extended 1 99))")
        .unwrap();
    session.collect().unwrap();
    for source in [
        "(instance? PersistentVector original)",
        "(== (count original) 3)",
        "(== (nth original 1) 20)",
        "(== (nth extended 1) 20)",
        "(== (nth changed 1) 99)",
        "(== (count (-pop extended)) 3)",
    ] {
        let value = session
            .eval(source)
            .unwrap_or_else(|error| panic!("{source}: {error:?}"));
        assert!(
            session
                .inspect(&value, |store, value| Ok(value
                    .unwrap_anyref()
                    .unwrap()
                    .as_i31(&store)?
                    .unwrap()
                    .get_u32()
                    == 4))
                .unwrap(),
            "{source}"
        );
    }
}

#[test]
fn compiled_macro_vectors_transport_actual_binding_vectors_and_quoted_symbols() {
    use suss_cli::portable_macros::CompiledMacros;
    let mut macros = CompiledMacros::new().unwrap();
    macros
        .define("(defmacro bind [x] (list 'let ['v x] 'v))")
        .unwrap();
    let mut runtime = Session::new_repl().unwrap();
    let value = runtime
        .eval_with_macros("(bind (+ 20 22))", &mut macros)
        .unwrap();
    let bits = runtime
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
    assert_eq!(bits, 42f64.to_bits());
}

#[test]
fn compiled_macro_vectors_preserve_trie_updates_and_transport_across_gc() {
    use suss_cli::portable_macro_data::FormBridge;
    use suss_reader::forms::Kind;
    let mut session = Session::new_macro().unwrap();
    let bridge = FormBridge::new(&mut session).unwrap();
    session.set_operation_fuel(100_000_000);
    session
        .eval("(def original (loop [v [] i 0] (if (< i 1100) (recur (-conj v i) (inc i)) v)))")
        .unwrap();
    session
        .eval("(def changed (-assoc-n original 1024 -1)) (def popped (-pop original))")
        .unwrap();
    session.collect().unwrap();
    let value = session.eval("original").unwrap();
    let form = bridge.read(&mut session, &value, 101..111).unwrap();
    let Kind::Vector(items) = form.kind else {
        panic!("Actual vector syntax");
    };
    assert_eq!(items.len(), 1100);
    for (i, item) in items.iter().enumerate() {
        assert!(matches!(item.kind, Kind::Number(n) if n == i as f64));
        assert_eq!(item.span, 101..111);
    }
    for source in [
        "(and (== (nth original 1024) 1024) (== (nth changed 1024) -1) (== (count popped) 1099))",
        "(== (nth (-pop (-pop (-pop (-pop original)))) 1055) 1055)",
    ] {
        let value = session.eval(source).unwrap();
        assert!(session
            .inspect(&value, |store, value| Ok(value
                .unwrap_anyref()
                .unwrap()
                .as_i31(&store)?
                .unwrap()
                .get_u32()
                == 4))
            .unwrap());
    }
}

#[test]
fn compiled_macro_vectors_quote_nested_data_and_reject_malformed_storage() {
    use suss_cli::portable_macro_data::FormBridge;
    use suss_reader::forms::{read_forms, Kind};
    let mut session = Session::new_macro().unwrap();
    let bridge = FormBridge::new(&mut session).unwrap();
    let original = read_forms("[alpha [:key (beta -0.0)] []]")
        .unwrap()
        .remove(0);
    let value = bridge.quote(&mut session, original).unwrap();
    session.collect().unwrap();
    let actual = bridge.read(&mut session, &value, 77..88).unwrap();
    let Kind::Vector(items) = actual.kind else {
        panic!("Vector syntax");
    };
    assert_eq!(items.len(), 3);
    assert!(matches!(items[0].kind, Kind::Symbol(_)));
    assert!(matches!(items[1].kind, Kind::Vector(_)));
    let Kind::Vector(nested) = &items[1].kind else {
        panic!("nested vector");
    };
    let Kind::List(list) = &nested[1].kind else {
        panic!("nested list");
    };
    let Kind::Number(n) = list[1].kind else {
        panic!("binary64");
    };
    assert_eq!(n.to_bits(), (-0f64).to_bits());
    for source in [
        "(PersistentVector. nil 2 5 (.-EMPTY_NODE PersistentVector) (array 1) nil)",
        "(PersistentVector. nil 1 6 (.-EMPTY_NODE PersistentVector) (array 1) nil)",
        "(PersistentVector. 1 0 5 (.-EMPTY_NODE PersistentVector) (array) nil)",
        "(PersistentVector. nil 0 5 (VectorNode. (js-obj) (make-array 32)) (array) nil)",
        "(PersistentVector. nil 0 5 (VectorNode. nil (make-array 31)) (array) nil)",
        "(deftype ForeignVector [meta cnt shift root tail hash]) (ForeignVector. nil 0 5 nil (array) nil)",
    ] {
        let value = session.eval(source).unwrap();
        assert!(bridge.read(&mut session, &value, 200..210).is_err(), "{source}");
    }
}

#[test]
fn compiled_macro_array_slice_clamps_bounds_and_owns_copied_storage() {
    let mut session = Session::new_macro().unwrap();
    session
        .eval("(def a (array 10 20 30 40)) (def copy (.slice a 1 -1)) (aset copy 0 99)")
        .unwrap();
    for source in [
        "(and (== (alength copy) 2) (== (aget copy 0) 99) (== (aget a 1) 20))",
        "(== (alength (.slice a -99 99)) 4)",
        "(== (alength (.slice a 3 1)) 0)",
        "(== (alength (.slice a 1.9 3.9)) 2)",
        "(== (alength (.slice a ##NaN ##Inf)) 4)",
        "(== (alength (.slice a ##-Inf ##-Inf)) 0)",
        "(== (alength (.slice a 0 nil)) 0)",
        "(== (alength (.slice a)) 4)",
        "(identical? (.-slice a) (.-slice copy))",
    ] {
        let value = session
            .eval(source)
            .unwrap_or_else(|error| panic!("{source}: {error:?}"));
        assert!(
            session
                .inspect(&value, |store, value| Ok(value
                    .unwrap_anyref()
                    .unwrap()
                    .as_i31(&store)?
                    .unwrap()
                    .get_u32()
                    == 4))
                .unwrap(),
            "{source}"
        );
    }
    session.eval("(def detached (.-slice a))").unwrap();
    assert!(session.eval("(detached 0 1)").is_err());
    assert!(session.eval("(.slice a 0 1 2)").is_err());
}

#[test]
fn compiled_macro_vectors_large_literals_use_retained_transient_factory() {
    use suss_cli::portable_macro_data::FormBridge;
    use suss_reader::forms::{read_forms, Kind};
    let mut session = Session::new_macro().unwrap();
    session.set_operation_fuel(100_000_000);
    let bridge = FormBridge::new(&mut session).unwrap();
    for size in [31, 32, 33, 65, 1057] {
        let source = format!(
            "[{}]",
            (0..size)
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join(" ")
        );
        let input = read_forms(&source).unwrap().remove(0);
        let value = bridge
            .quote(&mut session, input)
            .unwrap_or_else(|error| panic!("size{size}: {error:?}"));
        session.collect().unwrap();
        let form = bridge.read(&mut session, &value, 900..910).unwrap();
        let Kind::Vector(items) = form.kind else {
            panic!("Vector");
        };
        assert_eq!(items.len(), size);
        for (index, item) in items.iter().enumerate() {
            assert!(matches!(item.kind, Kind::Number(n) if n == index as f64));
        }
    }
    session.eval("(def old [10 20 30]) (def t (transient old)) (-assoc-n! t 1 99) (conj! t 40) (def result (persistent! t))").unwrap();
    for source in [
        "(== (nth old 1) 20)",
        "(== (nth result 1) 99)",
        "(== (count result) 4)",
    ] {
        let value = session.eval(source).unwrap();
        assert!(session
            .inspect(&value, |store, value| Ok(value
                .unwrap_anyref()
                .unwrap()
                .as_i31(&store)?
                .unwrap()
                .get_u32()
                == 4))
            .unwrap());
    }
    for source in [
        "(conj! t 1)",
        "(persistent! t)",
        "(count t)",
        "(nth t 0)",
        "(-assoc-n! t 0 1)",
        "(-pop! t)",
    ] {
        assert!(session.eval(source).is_err(), "{source}");
    }
}

#[test]
fn compiled_macro_vectors_match_fresh_pinned_scalar_observations_in_both_phases() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/macro-vector-cases.json"
    ))
    .unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 40);
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        for case in cases {
            let value = session
                .eval(case["source"].as_str().unwrap())
                .unwrap_or_else(|error| panic!("{}: {error:?}", case["id"]));
            session.collect().unwrap();
            let actual = session
                .inspect(&value, |mut store, value| {
                    Ok(if case["expected"]["tag"] == "bool" {
                        let n = value
                            .unwrap_anyref()
                            .unwrap()
                            .as_i31(&store)?
                            .unwrap()
                            .get_u32();
                        let v = match n {
                            2 => false,
                            4 => true,
                            _ => panic!("Boolean layout"),
                        };
                        serde_json::json!({"tag":"bool","value":v})
                    } else {
                        let fields = value
                            .unwrap_anyref()
                            .unwrap()
                            .as_struct(&store)?
                            .unwrap()
                            .fields(&mut store)?
                            .collect::<Vec<_>>();
                        let [wasmtime::Val::F64(bits)] = fields.as_slice() else {
                            panic!("Number layout");
                        };
                        serde_json::json!({"tag":"f64","bits":format!("{bits:016x}")})
                    })
                })
                .unwrap();
            assert_eq!(actual, case["expected"], "{}", case["id"]);
        }
    }
}
