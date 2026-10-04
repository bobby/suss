//! Execute actual portable AOT components through typed canonical host calls.
use std::sync::OnceLock;
use suss_compile::portable::{
    self,
    resolve::{Environment, Phase},
};
use suss_reader::Symbol;
use wasmtime::component::{Component, Linker};
use wasmtime::{AsContextMut, Config, Engine, Store};
use wit_parser::{Resolve, WorldId};

fn engine() -> Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE
        .get_or_init(|| {
            let mut config = Config::new();
            config
                .wasm_gc(true)
                .wasm_function_references(true)
                .wasm_tail_call(true)
                .wasm_exceptions(true)
                .wasm_component_model(true)
                .consume_fuel(true)
                .cranelift_opt_level(wasmtime::OptLevel::None);
            Engine::new(&config).unwrap()
        })
        .clone()
}
fn world(source: &str) -> (Resolve, WorldId) {
    let mut resolve = Resolve::new();
    let package = resolve.push_str("api.wit", source).unwrap();
    let world = *resolve.packages[package].worlds.values().next().unwrap();
    (resolve, world)
}
fn mappings(names: &[&str]) -> Vec<(String, Symbol)> {
    names
        .iter()
        .map(|name| (name.to_string(), Symbol::new(*name)))
        .collect()
}

#[test]
fn two_wit_exports_can_share_one_live_source_cell() {
    let fragment = portable::prepare_fragment(
        "(def shared (fn [x] (+ x 1)))",
        &Environment::default(),
        Phase::Runtime,
    )
    .unwrap();
    let (resolve, selected) = world(
        "package test:aliases; world api { export first: func(x: f64) -> f64; export second: func(x: f64) -> f64; }",
    );
    let bytes = portable::aot::component(
        &[fragment],
        &resolve,
        selected,
        &[
            ("first".into(), Symbol::new("shared")),
            ("second".into(), Symbol::new("shared")),
        ],
    )
    .unwrap();
    let engine = engine();
    let component = Component::new(&engine, bytes).unwrap();
    let mut store = Store::new(&engine, ());
    store.set_fuel(20_000_000).unwrap();
    let instance = Linker::new(&engine)
        .instantiate(&mut store, &component)
        .unwrap();
    for name in ["first", "second"] {
        let function = instance
            .get_typed_func::<(f64,), (f64,)>(&mut store, name)
            .unwrap();
        assert_eq!(function.call(&mut store, (41.0,)).unwrap().0, 42.0);
        function.post_return(&mut store).unwrap();
    }
}

#[test]
fn portable_component_keeps_source_order_old_captures_live_cells_and_scalar_values() {
    let first = portable::prepare_fragment(
        "(ns app) (def effects 0) (def f (fn [x] (do (set! effects (+ effects 1)) (+ x 2)))) (def old f)",
        &Environment::default(), Phase::Runtime,
    ).unwrap();
    let second = portable::prepare_fragment(
        "(def f (fn [x] (+ x 100))) (def old-call (fn [x] (old x))) (def current (fn [x] (f x))) (def seen (fn [] effects)) (def echo (fn [x] x)) (def echo32 (fn [x] x)) (def round (fn [x] x)) (def invert (fn [x] (if x false true))) (def bump (fn [] (set! effects (+ effects 10)))) (set! effects (+ effects 1))",
        &first.environment, Phase::Runtime,
    ).unwrap();
    let (resolve, selected) = world(
        "package test:portable; world api { export old-call: func(x: f64) -> f64; export current: func(x: f64) -> f64; export seen: func() -> f64; export echo: func(x: f64) -> f64; export echo32: func(x: f32) -> f32; export round: func(x: f64) -> f32; export invert: func(x: bool) -> bool; export bump: func(); }",
    );
    let bytes = portable::aot::component(
        &[first, second],
        &resolve,
        selected,
        &mappings(&[
            "old-call", "current", "seen", "echo", "echo32", "round", "invert", "bump",
        ]),
    )
    .unwrap();
    let mut imports = 0;
    for payload in wasmparser::Parser::new(0).parse_all(&bytes) {
        if let wasmparser::Payload::ComponentImportSection(section) = payload.unwrap() {
            imports += section.count();
        }
    }
    assert_eq!(
        imports, 0,
        "pure selected world must not gain hidden host imports"
    );
    let engine = engine();
    let component = Component::new(&engine, bytes).unwrap();
    for _ in 0..2 {
        let mut store = Store::new(&engine, ());
        store.set_fuel(20_000_000).unwrap();
        let instance = Linker::new(&engine)
            .instantiate(&mut store, &component)
            .unwrap();
        let seen = instance
            .get_typed_func::<(), (f64,)>(&mut store, "seen")
            .unwrap();
        assert_eq!(
            seen.call(&mut store, ()).unwrap().0.to_bits(),
            1.0_f64.to_bits()
        );
        seen.post_return(&mut store).unwrap();
        let old = instance
            .get_typed_func::<(f64,), (f64,)>(&mut store, "old-call")
            .unwrap();
        assert_eq!(
            old.call(&mut store, (3.0,)).unwrap().0.to_bits(),
            5.0_f64.to_bits()
        );
        old.post_return(&mut store).unwrap();
        store.gc(None).unwrap();
        let current = instance
            .get_typed_func::<(f64,), (f64,)>(&mut store, "current")
            .unwrap();
        assert_eq!(
            current.call(&mut store, (3.0,)).unwrap().0.to_bits(),
            103.0_f64.to_bits()
        );
        current.post_return(&mut store).unwrap();
        assert_eq!(
            seen.call(&mut store, ()).unwrap().0.to_bits(),
            2.0_f64.to_bits()
        );
        seen.post_return(&mut store).unwrap();
        let bump = instance
            .get_typed_func::<(), ()>(&mut store, "bump")
            .unwrap();
        bump.call(&mut store, ()).unwrap();
        bump.post_return(&mut store).unwrap();
        assert_eq!(
            seen.call(&mut store, ()).unwrap().0.to_bits(),
            12.0_f64.to_bits()
        );
        seen.post_return(&mut store).unwrap();
        let echo = instance
            .get_typed_func::<(f64,), (f64,)>(&mut store, "echo")
            .unwrap();
        for value in [
            -0.0,
            f64::from_bits(1),
            f64::MAX,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ] {
            assert_eq!(
                echo.call(&mut store, (value,)).unwrap().0.to_bits(),
                value.to_bits()
            );
            echo.post_return(&mut store).unwrap();
        }
        let echo32 = instance
            .get_typed_func::<(f32,), (f32,)>(&mut store, "echo32")
            .unwrap();
        for value in [
            -0.0_f32,
            f32::from_bits(1),
            f32::MAX,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ] {
            assert_eq!(
                echo32.call(&mut store, (value,)).unwrap().0.to_bits(),
                value.to_bits()
            );
            echo32.post_return(&mut store).unwrap();
        }
        let round = instance
            .get_typed_func::<(f64,), (f32,)>(&mut store, "round")
            .unwrap();
        assert_eq!(
            round.call(&mut store, (0.1,)).unwrap().0.to_bits(),
            0.1_f32.to_bits()
        );
        round.post_return(&mut store).unwrap();
        let invert = instance
            .get_typed_func::<(bool,), (bool,)>(&mut store, "invert")
            .unwrap();
        for value in [false, true] {
            assert_eq!(invert.call(&mut store, (value,)).unwrap().0, !value);
            invert.post_return(&mut store).unwrap();
        }
    }
}

#[test]
fn portable_component_boundary_rejects_wrong_result_with_language_payload() {
    // Cover the i31 boolean sentinel guard, non-i31 guard, and numeric cast guard.
    for (result, value) in [
        ("bool", "nil"),
        ("bool", "1"),
        ("f32", "false"),
        ("f64", "nil"),
    ] {
        let fragment = portable::prepare_fragment(
            &format!("(def bad (fn [] {value}))"),
            &Environment::default(),
            Phase::Runtime,
        )
        .unwrap();
        let (resolve, selected) = world(&format!(
            "package test:invalid; world api {{ export bad: func() -> {result}; }}"
        ));
        let bytes =
            portable::aot::component(&[fragment], &resolve, selected, &mappings(&["bad"])).unwrap();
        let engine = engine();
        let mut store = Store::new(&engine, ());
        store.set_fuel(20_000_000).unwrap();
        let component = Component::new(&engine, bytes).unwrap();
        let instance = Linker::new(&engine)
            .instantiate(&mut store, &component)
            .unwrap();
        match result {
            "bool" => {
                let bad = instance
                    .get_typed_func::<(), (bool,)>(&mut store, "bad")
                    .unwrap();
                assert!(bad.call(&mut store, ()).is_err());
            }
            "f32" => {
                let bad = instance
                    .get_typed_func::<(), (f32,)>(&mut store, "bad")
                    .unwrap();
                assert!(bad.call(&mut store, ()).is_err());
            }
            "f64" => {
                let bad = instance
                    .get_typed_func::<(), (f64,)>(&mut store, "bad")
                    .unwrap();
                assert!(bad.call(&mut store, ()).is_err());
            }
            _ => unreachable!(),
        }
        let exception = store
            .as_context_mut()
            .take_pending_exception()
            .expect("boundary failure must have language payload");
        let fields = exception.fields(&mut store).unwrap().collect::<Vec<_>>();
        assert_eq!(fields.len(), 1);
        let payload = fields[0]
            .unwrap_anyref()
            .unwrap()
            .as_struct(&store)
            .unwrap()
            .unwrap();
        let message = payload.field(&mut store, 1).unwrap();
        let array = message
            .unwrap_anyref()
            .unwrap()
            .as_array(&store)
            .unwrap()
            .unwrap();
        let units = array
            .elems(&mut store)
            .unwrap()
            .map(|unit| unit.unwrap_i32() as u16)
            .collect::<Vec<_>>();
        assert_eq!(
            String::from_utf16(&units).unwrap(),
            "WIT export bad returned an incompatible scalar value"
        );
    }
}

#[test]
fn portable_component_initialization_preserves_first_language_throw_payload() {
    let first = portable::prepare_fragment(
        "(def f (fn [] 42)) (throw 17)",
        &Environment::default(),
        Phase::Runtime,
    )
    .unwrap();
    let second =
        portable::prepare_fragment("(throw 99)", &first.environment, Phase::Runtime).unwrap();
    let (resolve, selected) =
        world("package test:initializers; world api { export f: func() -> f64; }");
    let bytes =
        portable::aot::component(&[first, second], &resolve, selected, &mappings(&["f"])).unwrap();
    let engine = engine();
    let component = Component::new(&engine, bytes).unwrap();
    let mut store = Store::new(&engine, ());
    store.set_fuel(20_000_000).unwrap();
    assert!(
        Linker::new(&engine)
            .instantiate(&mut store, &component)
            .is_err()
    );
    // Inspect the actual Wasm exception instead of accepting an arbitrary trap.
    // If the later initializer ran first, its independently decoded payload
    // would be99 instead of the original source exception17.
    let exception = store
        .as_context_mut()
        .take_pending_exception()
        .expect("source language exception");
    let fields = exception.fields(&mut store).unwrap().collect::<Vec<_>>();
    assert_eq!(fields.len(), 1);
    let payload = fields[0]
        .unwrap_anyref()
        .unwrap()
        .as_struct(&store)
        .unwrap()
        .unwrap();
    assert_eq!(
        payload.field(&mut store, 0).unwrap().unwrap_f64().to_bits(),
        17.0_f64.to_bits()
    );
}

#[test]
fn portable_component_preserves_mixed_scalar_argument_order_and_nan_values() {
    let fragment = portable::prepare_fragment(
        "(def choose (fn [x flag y] (if flag (+ x y) (- x y)))) (def echo (fn [x] x))",
        &Environment::default(),
        Phase::Runtime,
    )
    .unwrap();
    let (resolve, selected) = world(
        "package test:mixed; world api { export choose: func(x: f32, flag: bool, y: f64) -> f64; export echo: func(x: f64) -> f32; }",
    );
    let bytes = portable::aot::component(
        &[fragment],
        &resolve,
        selected,
        &mappings(&["choose", "echo"]),
    )
    .unwrap();
    let engine = engine();
    let component = Component::new(&engine, bytes).unwrap();
    let mut store = Store::new(&engine, ());
    store.set_fuel(20_000_000).unwrap();
    let instance = Linker::new(&engine)
        .instantiate(&mut store, &component)
        .unwrap();
    let choose = instance
        .get_typed_func::<(f32, bool, f64), (f64,)>(&mut store, "choose")
        .unwrap();
    for (flag, expected) in [(false, 0.1_f32 as f64 - 3.0), (true, 0.1_f32 as f64 + 3.0)] {
        assert_eq!(
            choose
                .call(&mut store, (0.1, flag, 3.0))
                .unwrap()
                .0
                .to_bits(),
            expected.to_bits()
        );
        choose.post_return(&mut store).unwrap();
    }
    store.gc(None).unwrap();
    let echo = instance
        .get_typed_func::<(f64,), (f32,)>(&mut store, "echo")
        .unwrap();
    for value in [f64::NAN, f64::from_bits(0xfff8_0000_0000_0123)] {
        assert!(echo.call(&mut store, (value,)).unwrap().0.is_nan());
        echo.post_return(&mut store).unwrap();
    }
}

#[test]
fn portable_component_rejects_macro_phase_artifact_before_assembly() {
    let runtime = portable::prepare_fragment(
        "(def f (fn [x] x))",
        &Environment::default(),
        Phase::Runtime,
    )
    .unwrap();
    let macro_fragment =
        portable::prepare_fragment("(def m (fn [x] x))", &Environment::default(), Phase::Macro)
            .unwrap();
    let (resolve, selected) =
        world("package test:phase; world api { export f: func(x: f64) -> f64; }");
    // The final Runtime catalog resolves the export successfully. Rejection must
    // therefore come from the preceding artifact's actual phase identity.
    let error = portable::aot::component(
        &[macro_fragment, runtime],
        &resolve,
        selected,
        &mappings(&["f"]),
    )
    .unwrap_err();
    assert_eq!(error.message, "Source artifact phase identity mismatch");
}

#[test]
fn portable_component_rejects_missing_duplicate_unknown_and_unsupported_mappings() {
    let fragment = portable::prepare_fragment(
        "(def f (fn [x] x))",
        &Environment::default(),
        Phase::Runtime,
    )
    .unwrap();
    let (resolve, selected) =
        world("package test:mapping; world api { export f: func(x: f64) -> f64; }");
    for mapping in [vec![], mappings(&["f", "f"]), mappings(&["f", "extra"])] {
        assert!(
            portable::aot::component(
                std::slice::from_ref(&fragment),
                &resolve,
                selected,
                &mapping
            )
            .is_err()
        );
    }
    let (resolve, selected) =
        world("package test:unsupported; world api { export f: func(x: string) -> string; }");
    assert!(
        portable::aot::component(
            std::slice::from_ref(&fragment),
            &resolve,
            selected,
            &mappings(&["f"])
        )
        .unwrap_err()
        .message
        .contains("bool/f32/f64")
    );
    let (resolve, selected) = world(
        "package test:imports; world api { import host: func(); export f: func(x: f64) -> f64; }",
    );
    assert!(
        portable::aot::component(
            std::slice::from_ref(&fragment),
            &resolve,
            selected,
            &mappings(&["f"])
        )
        .unwrap_err()
        .message
        .contains("imported WIT")
    );
}
