//! Execute the new reader scalar boundary through production ABI intrinsics.
//! This does not claim that the legacy compiler lowers portable forms yet.
mod support;
use suss_compile::runtime_abi;
use suss_reader::forms::{Kind, read_forms};
use wasmtime::{Instance, Module, Store, Val};

#[test]
fn portable_reader_matches_the_pinned_scalar_corpus() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/oracle/reader-cases.json")).unwrap();
    assert_eq!(corpus["schema"], 1);
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    let mut ids = std::collections::HashSet::new();
    for case in corpus["cases"].as_array().unwrap() {
        assert!(ids.insert(case["id"].as_str().unwrap()));
        let source = case["source"].as_str().unwrap();
        let forms = read_forms(source).unwrap();
        assert_eq!(forms.len(), 1);
        let value = match &forms[0].kind {
            Kind::Number(number) => {
                serde_json::json!({"tag":"f64", "bits":format!("{:016x}", number.to_bits())})
            }
            Kind::String(units) => serde_json::json!({"tag":"string", "units":units}),
            _ => panic!("unknown scalar reader observation"),
        };
        assert_eq!(value, case["expected"], "{source}");
    }
    assert_eq!(ids.len(), 14);
}

#[test]
fn portable_reader_scalars_transfer_losslessly_into_shared_runtime() {
    let engine = support::engine();
    let bytes = runtime_abi::module();
    runtime_abi::verify_artifact(&bytes, &runtime_abi::Manifest::default()).unwrap();
    let module = Module::new(&engine, bytes).unwrap();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(&mut store, &module, &[]).unwrap();
    let boxed = runtime.get_func(&mut store, "number-box").unwrap();
    for (source, expected) in [
        ("9007199254740993", 9007199254740992.0f64),
        ("-0.0", -0.0),
        ("1e3", 1000.0),
        ("##Inf", f64::INFINITY),
        ("##-Inf", f64::NEG_INFINITY),
    ] {
        let forms = read_forms(source).unwrap();
        let Kind::Number(value) = forms[0].kind else {
            panic!("expected ordinary number")
        };
        let mut result = [Val::null_any_ref()];
        boxed
            .call(&mut store, &[Val::F64(value.to_bits())], &mut result)
            .unwrap();
        store.gc(None).unwrap();
        let Val::AnyRef(Some(reference)) = &result[0] else {
            panic!("expected boxed number")
        };
        let object = reference.as_struct(&store).unwrap().unwrap();
        let fields: Vec<_> = object.fields(&mut store).unwrap().collect();
        assert!(matches!(fields.as_slice(), [Val::F64(bits)] if *bits == expected.to_bits()));
    }
    let source = r#""\uD800\u0000😀\uFFFF""#;
    let forms = read_forms(source).unwrap();
    let Kind::String(units) = &forms[0].kind else {
        panic!("expected UTF-16 string")
    };
    assert_eq!(units, &[0xd800, 0, 0xd83d, 0xde00, 0xffff]);
    let new = runtime.get_func(&mut store, "string-new").unwrap();
    let set = runtime.get_func(&mut store, "string-set-unit").unwrap();
    let mut string = [Val::null_any_ref()];
    new.call(&mut store, &[Val::I32(units.len() as i32)], &mut string)
        .unwrap();
    for (index, unit) in units.iter().enumerate() {
        let mut accepted = [Val::I32(0)];
        set.call(
            &mut store,
            &[
                string[0].clone(),
                Val::I32(index as i32),
                Val::I32(i32::from(*unit)),
            ],
            &mut accepted,
        )
        .unwrap();
        assert_eq!(accepted[0].i32(), Some(1));
    }
    store.gc(None).unwrap();
    let Val::AnyRef(Some(reference)) = &string[0] else {
        panic!("expected GC string")
    };
    let array = reference.as_array(&store).unwrap().unwrap();
    assert_eq!(array.len(&store).unwrap(), units.len() as u32);
    for (index, expected) in units.iter().enumerate() {
        assert_eq!(
            array.get(&mut store, index as u32).unwrap().i32(),
            Some(i32::from(*expected))
        );
    }
}
