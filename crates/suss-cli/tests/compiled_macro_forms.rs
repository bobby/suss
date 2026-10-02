use suss_cli::portable_session::{Session, SessionError};
use suss_compile::portable::resolve::Phase;
use suss_reader::forms::{read_forms, Form, Kind};
use wasmtime::Val;

#[test]
fn compiled_macro_forms_preserve_utf16_and_binary64_without_source_roundtrip() {
    let mut session = Session::new_macro().unwrap();
    let text = Form {
        span: 81..92,
        metadata: vec![],
        kind: Kind::String(vec![0xd800, 0, 0xd83d, 0xde00]),
    };
    let value = session.eval_forms(vec![text], 80..100).unwrap();
    session.collect().unwrap();
    let units = session
        .inspect(&value, |mut store, value| {
            let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            Ok(array
                .elems(&mut store)?
                .map(|v| v.unwrap_i32() as u16)
                .collect::<Vec<_>>())
        })
        .unwrap();
    assert_eq!(units, [0xd800, 0, 0xd83d, 0xde00]);
    for bits in [
        (-0f64).to_bits(),
        0x7ff8_0000_0000_0001,
        f64::INFINITY.to_bits(),
    ] {
        let form = Form {
            span: 201..204,
            metadata: vec![],
            kind: Kind::Number(f64::from_bits(bits)),
        };
        let value = session.eval_forms(vec![form], 200..210).unwrap();
        let actual = session
            .inspect(&value, |mut store, value| {
                let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                let fields = object.fields(&mut store)?.collect::<Vec<_>>();
                let [Val::F64(bits)] = fields.as_slice() else {
                    panic!("Expected exact Number layout");
                };
                Ok(*bits)
            })
            .unwrap();
        assert_eq!(actual, bits);
    }
}

#[test]
fn compiled_macro_forms_keep_locations_metadata_and_compile_atomicity() {
    let mut session = Session::new_macro().unwrap();
    session.eval("(def existing 7)").unwrap();
    let before = session.stats();
    let mut forms = read_forms("(def ghost 1) missing").unwrap();
    forms[1].span = 501..508;
    let Err(SessionError::Compile(error)) = session.eval_forms(forms, 400..600) else {
        panic!("Expected located failure");
    };
    assert_eq!(error.span, 501..508);
    assert_eq!(session.stats(), before);
    assert!(matches!(
        session.eval("ghost"),
        Err(SessionError::Compile(_))
    ));
    let mut forms = read_forms("(quote ^:private x)").unwrap();
    let Kind::List(items) = &mut forms[0].kind else {
        panic!("quote");
    };
    items[1].span = 701..720;
    items[1].metadata[0].span = 701..720;
    items[1].metadata[0].kind = Kind::Number(1.0);
    let Err(SessionError::Compile(error)) = session.eval_forms(forms, 700..730) else {
        panic!("Invalid metadata must not be discarded");
    };
    assert_eq!(error.span, 701..720);
    assert!(error.message.contains("metadata"));
    assert_eq!(session.stats(), before);
    assert_eq!(session.phase(), Phase::Macro);
}

#[test]
fn compiled_macro_forms_use_phase_dependencies_and_conditional_selection_once() {
    use suss_cli::portable_session::SessionOptions;
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("dep.sus"), "(ns dep) (def value 37)").unwrap();
    let mut session = Session::with_options_in(
        SessionOptions {
            source_paths: vec![root.path().to_owned()],
            ..Default::default()
        },
        Phase::Macro,
    )
    .unwrap();
    let forms =
        read_forms("(ns caller (:require [dep :as d])) #?(:suss d/value :cljs missing)").unwrap();
    let value = session.eval_forms(forms, 0..80).unwrap();
    let bits = session
        .inspect(&value, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let fields = object.fields(&mut store)?.collect::<Vec<_>>();
            let [Val::F64(bits)] = fields.as_slice() else {
                panic!("Number");
            };
            Ok(*bits)
        })
        .unwrap();
    assert_eq!(bits, 37f64.to_bits());
    assert_eq!(session.current_namespace(), "caller");
    assert_eq!(session.stats().loaded_modules, 1);
    session
        .eval_forms(
            read_forms("#?(:cljs d/value :suss missing)").unwrap(),
            0..50,
        )
        .unwrap();
    assert_eq!(session.stats().loaded_modules, 1);
}

#[test]
fn compiled_macro_forms_execute_actual_transformer_output_through_runtime_pipeline() {
    use suss_cli::portable_macro_data::FormBridge;
    let mut macros = Session::new_macro().unwrap();
    let mut runtime = Session::new_repl().unwrap();
    let bridge = FormBridge::new(&mut macros).unwrap();
    macros
        .eval("(def effects 0) (def expand (fn [x] (set! effects (+ effects 1)) (list '+ x 1)))")
        .unwrap();
    let transformer = macros.eval("expand").unwrap();
    let argument = bridge
        .quote(&mut macros, read_forms("41").unwrap().remove(0))
        .unwrap();
    let result = macros.invoke(&transformer, &[&argument]).unwrap();
    macros.collect().unwrap();
    let expanded = bridge.read(&mut macros, &result, 801..820).unwrap();
    assert_eq!(expanded.span, 801..820);
    let Kind::List(items) = &expanded.kind else {
        panic!("Expected actual List syntax");
    };
    assert_eq!(items.len(), 3);
    assert!(items.iter().all(|form| form.span == (801..820)));
    let value = runtime.eval_forms(vec![expanded], 801..820).unwrap();
    let bits = runtime
        .inspect(&value, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let fields = object.fields(&mut store)?.collect::<Vec<_>>();
            let [Val::F64(bits)] = fields.as_slice() else {
                panic!("Number");
            };
            Ok(*bits)
        })
        .unwrap();
    assert_eq!(bits, 42f64.to_bits());
    let effects = macros.eval("effects").unwrap();
    let count = macros
        .inspect(&effects, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let fields = object.fields(&mut store)?.collect::<Vec<_>>();
            let [Val::F64(bits)] = fields.as_slice() else {
                panic!("Number");
            };
            Ok(*bits)
        })
        .unwrap();
    assert_eq!(
        count,
        1f64.to_bits(),
        "Transformer body is not replayed during transport or runtime compilation"
    );
    assert!(matches!(
        bridge.read(&mut runtime, &result, 0..1),
        Err(SessionError::ForeignValue)
    ));
    macros.reset().unwrap();
    let value = macros.eval("'x").unwrap();
    assert!(matches!(
        bridge.read(&mut macros, &value, 0..1),
        Err(SessionError::ForeignValue)
    ));
}

#[test]
fn compiled_macro_forms_reject_unknown_malformed_metadata_and_bounded_data() {
    use suss_cli::portable_macro_data::FormBridge;
    let mut macros = Session::new_macro().unwrap();
    let bridge = FormBridge::new(&mut macros).unwrap();
    for (source, message) in [
        ("(fn [] 1)", "layout"),
        ("(atom 1)", "nominal"),
        ("(new suss.core/List nil 1 nil 99 nil)", "count"),
        ("(new suss.core/Symbol nil 1 \"x\" 0 nil)", "UTF16"),
        ("(with-meta '(x) 1)", "metadata"),
        (
            "(new suss.core/Symbol nil \"\\uD800\" \"x\" 0 nil)",
            "surrogate",
        ),
    ] {
        let value = macros.eval(source).unwrap();
        let before = macros.stats();
        let Err(SessionError::Compile(error)) = bridge.read(&mut macros, &value, 901..922) else {
            panic!("Must reject {source}");
        };
        assert_eq!(error.span, 901..922);
        assert!(error.message.contains(message), "{error}");
        assert_eq!(macros.stats(), before);
    }
    let old = macros.eval("'x").unwrap();
    macros
        .eval("(ns suss.core) (def Symbol (fn [] 0))")
        .unwrap();
    macros.enter_namespace("user").unwrap();
    macros.collect().unwrap();
    let form = bridge.read(&mut macros, &old, 1..2).unwrap();
    let Kind::Symbol(symbol) = form.kind else {
        panic!("Captured class identity");
    };
    assert_eq!(symbol.name, "x");
    let empty = macros.eval("'()").unwrap();
    assert!(
        matches!(bridge.read(&mut macros, &empty, 0..1).unwrap().kind, Kind::List(items) if items.is_empty())
    );
    let cons = macros.eval("(cons 1 (cons 2 nil))").unwrap();
    assert!(
        matches!(bridge.read(&mut macros, &cons, 0..1).unwrap().kind, Kind::List(items) if items.len() == 2)
    );
}

#[test]
fn compiled_macro_forms_bound_cycles_and_total_utf16_allocation() {
    use suss_cli::portable_macro_data::FormBridge;
    let mut macros = Session::new_macro().unwrap();
    let bridge = FormBridge::new(&mut macros).unwrap();
    let cycle = macros.eval("(cons 1 nil)").unwrap();
    macros
        .inspect(&cycle, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let data = object
                .field(&mut store, 1)?
                .unwrap_anyref()
                .unwrap()
                .as_array(&store)?
                .unwrap();
            data.set(&mut store, 2, value)?;
            Ok(())
        })
        .unwrap();
    let Err(SessionError::Compile(error)) = bridge.read(&mut macros, &cycle, 10..20) else {
        panic!("Cyclic syntax must fail within traversal bound");
    };
    assert!(error.message.contains("4096"));
    let wide = macros
        .eval(&format!("(list {})", vec!["1"; 128].join(" ")))
        .unwrap();
    macros.collect().unwrap();
    assert!(
        matches!(bridge.read(&mut macros, &wide, 0..1).unwrap().kind, Kind::List(items) if items.len() == 128)
    );
    let deep = macros
        .eval("(loop [i 0 s nil] (if (< i 65) (recur (+ i 1) (list s)) s))")
        .unwrap();
    let Err(SessionError::Compile(error)) = bridge.read(&mut macros, &deep, 0..1) else {
        panic!("Runtime nesting must be bounded independently of source nesting");
    };
    assert!(error.message.contains("nesting exceeds 64"));
    let strings = macros.eval("(list \"a\" \"b\")").unwrap();
    macros
        .inspect(&strings, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let data = object
                .field(&mut store, 1)?
                .unwrap_anyref()
                .unwrap()
                .as_array(&store)?
                .unwrap();
            let text = data
                .get(&mut store, 1)?
                .unwrap_anyref()
                .unwrap()
                .as_array(&store)?
                .unwrap();
            let ty = text.ty(&store)?;
            let allocator = wasmtime::ArrayRefPre::new(&mut store, ty);
            let large = wasmtime::ArrayRef::new(&mut store, &allocator, &Val::I32(97), 524_289)?;
            data.set(&mut store, 1, Val::AnyRef(Some(large.to_anyref())))?;
            let tail = data
                .get(&mut store, 2)?
                .unwrap_anyref()
                .unwrap()
                .as_struct(&store)?
                .unwrap();
            let data = tail
                .field(&mut store, 1)?
                .unwrap_anyref()
                .unwrap()
                .as_array(&store)?
                .unwrap();
            data.set(&mut store, 1, Val::AnyRef(Some(large.to_anyref())))?;
            Ok(())
        })
        .unwrap();
    let Err(SessionError::Compile(error)) = bridge.read(&mut macros, &strings, 21..30) else {
        panic!("Total copied UTF16 storage must be bounded");
    };
    assert!(error.message.contains("total UTF16"));
    let value = macros.eval("'ok").unwrap();
    assert!(
        matches!(bridge.read(&mut macros, &value, 0..1).unwrap().kind, Kind::Symbol(symbol) if symbol.name == "ok")
    );
}
