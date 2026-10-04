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
        ("u8", "false"),
        ("s8", "nil"),
        ("u16", "false"),
        ("s16", "nil"),
        ("u32", "false"),
        ("s32", "nil"),
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
            "u8" => {
                assert!(
                    instance
                        .get_typed_func::<(), (u8,)>(&mut store, "bad")
                        .unwrap()
                        .call(&mut store, ())
                        .is_err()
                );
            }
            "s8" => {
                assert!(
                    instance
                        .get_typed_func::<(), (i8,)>(&mut store, "bad")
                        .unwrap()
                        .call(&mut store, ())
                        .is_err()
                );
            }
            "u16" => {
                assert!(
                    instance
                        .get_typed_func::<(), (u16,)>(&mut store, "bad")
                        .unwrap()
                        .call(&mut store, ())
                        .is_err()
                );
            }
            "s16" => {
                assert!(
                    instance
                        .get_typed_func::<(), (i16,)>(&mut store, "bad")
                        .unwrap()
                        .call(&mut store, ())
                        .is_err()
                );
            }
            "u32" => {
                assert!(
                    instance
                        .get_typed_func::<(), (u32,)>(&mut store, "bad")
                        .unwrap()
                        .call(&mut store, ())
                        .is_err()
                );
            }
            "s32" => {
                assert!(
                    instance
                        .get_typed_func::<(), (i32,)>(&mut store, "bad")
                        .unwrap()
                        .call(&mut store, ())
                        .is_err()
                );
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

#[test]
fn small_integer_boundaries_round_trip_and_check_numeric_results() {
    let fragment = portable::prepare_fragment(
        "(def calls 0) (def echo (fn [x] (do (set! calls (+ calls 1)) x))) (def seen (fn [] calls))",
        &Environment::default(),
        Phase::Runtime,
    )
    .unwrap();
    let kinds = ["u8", "s8", "u16", "s16", "u32", "s32"];
    let declarations = kinds.iter().map(|kind| format!("export echo-{kind}: func(x: {kind}) -> {kind}; export check-{kind}: func(x: f64) -> {kind};")).collect::<String>();
    let (resolve, selected) = world(&format!(
        "package test:integers; world api {{ export seen: func() -> f64; {declarations} }}"
    ));
    let mut mappings = kinds
        .iter()
        .flat_map(|kind| [format!("echo-{kind}"), format!("check-{kind}")])
        .map(|name| (name, Symbol::new("echo")))
        .collect::<Vec<_>>();
    mappings.push(("seen".into(), Symbol::new("seen")));
    let bytes = portable::aot::component(&[fragment], &resolve, selected, &mappings).unwrap();
    let engine = engine();
    let component = Component::new(&engine, bytes).unwrap();
    let mut store = Store::new(&engine, ());
    store.set_fuel(50_000_000).unwrap();
    let instance = Linker::new(&engine)
        .instantiate(&mut store, &component)
        .unwrap();
    macro_rules! check {
        ($kind:literal, $ty:ty, $values:expr) => {{
            let echo = instance
                .get_typed_func::<($ty,), ($ty,)>(&mut store, concat!("echo-", $kind))
                .unwrap();
            let checked = instance
                .get_typed_func::<(f64,), ($ty,)>(&mut store, concat!("check-", $kind))
                .unwrap();
            for value in $values {
                assert_eq!(echo.call(&mut store, (value,)).unwrap().0, value);
                echo.post_return(&mut store).unwrap();
                assert_eq!(checked.call(&mut store, (value as f64,)).unwrap().0, value);
                checked.post_return(&mut store).unwrap();
            }
            store.gc(None).unwrap();
            assert_eq!(checked.call(&mut store, (-0.0,)).unwrap().0, 0);
            checked.post_return(&mut store).unwrap();
            for value in [
                <$ty>::MIN as f64 - 1.0,
                <$ty>::MAX as f64 + 1.0,
                0.5,
                -0.5,
                f64::from_bits(1),
                -f64::from_bits(1),
                f64::NAN,
                f64::INFINITY,
                f64::NEG_INFINITY,
            ] {
                // A failed lifted component call locks its Store in the pinned
                // canonical engine. Decode each failure in a fresh instance.
                let mut store = Store::new(&engine, ());
                store.set_fuel(20_000_000).unwrap();
                let instance = Linker::new(&engine)
                    .instantiate(&mut store, &component)
                    .unwrap();
                let checked = instance
                    .get_typed_func::<(f64,), ($ty,)>(&mut store, concat!("check-", $kind))
                    .unwrap();
                assert!(
                    checked.call(&mut store, (value,)).is_err(),
                    "{} accepted {value}",
                    $kind
                );
                let exception = store
                    .as_context_mut()
                    .take_pending_exception()
                    .expect("integer failure must carry a language exception");
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
                    concat!(
                        "WIT export check-",
                        $kind,
                        " returned an incompatible scalar value"
                    )
                );
            }
        }};
    }
    check!("u8", u8, [0, 1, 127, 128, u8::MAX]);
    check!("s8", i8, [i8::MIN, -1, 0, 1, i8::MAX]);
    check!("u16", u16, [0, 1, 32767, 32768, u16::MAX]);
    check!("s16", i16, [i16::MIN, -1, 0, 1, i16::MAX]);
    check!("u32", u32, [0, 1, 2147483647, 2147483648, u32::MAX]);
    check!("s32", i32, [i32::MIN, -1, 0, 1, i32::MAX]);
    let seen = instance
        .get_typed_func::<(), (f64,)>(&mut store, "seen")
        .unwrap();
    assert_eq!(
        seen.call(&mut store, ()).unwrap().0.to_bits(),
        66.0_f64.to_bits()
    );
    seen.post_return(&mut store).unwrap();
}

#[test]
fn small_integer_parameters_keep_mixed_scalar_positions_and_once_only_calls() {
    let fragment = portable::prepare_fragment(
        "(def calls 0) (def mixed (fn [a b c d e f flag tiny wide] (do (set! calls (+ calls 1)) (if flag (+ a (* b 2) (* c 4) (* d 8) (* e 16) (* f 32) tiny wide) (- a (* b 2) (* c 4) (* d 8) (* e 16) (* f 32) tiny wide))))) (def seen (fn [] calls))",
        &Environment::default(), Phase::Runtime,
    ).unwrap();
    let (resolve, selected) = world(
        "package test:mixed-integers; world api { export mixed: func(a: u8, b: s8, c: u16, d: s16, e: u32, f: s32, flag: bool, tiny: f32, wide: f64) -> f64; export seen: func() -> f64; }",
    );
    let bytes = portable::aot::component(
        &[fragment],
        &resolve,
        selected,
        &mappings(&["mixed", "seen"]),
    )
    .unwrap();
    let engine = engine();
    let component = Component::new(&engine, bytes).unwrap();
    let mut store = Store::new(&engine, ());
    store.set_fuel(20_000_000).unwrap();
    let instance = Linker::new(&engine)
        .instantiate(&mut store, &component)
        .unwrap();
    let mixed = instance
        .get_typed_func::<(u8, i8, u16, i16, u32, i32, bool, f32, f64), (f64,)>(&mut store, "mixed")
        .unwrap();
    let terms = [
        255.0,
        -128.0 * 2.0,
        65535.0 * 4.0,
        -32768.0 * 8.0,
        u32::MAX as f64 * 16.0,
        i32::MIN as f64 * 32.0,
        0.5,
        -0.25,
    ];
    for flag in [true, false] {
        let expected =
            terms[1..].iter().fold(
                terms[0],
                |value, next| if flag { value + next } else { value - next },
            );
        assert_eq!(
            mixed
                .call(
                    &mut store,
                    (
                        255,
                        -128,
                        65535,
                        -32768,
                        u32::MAX,
                        i32::MIN,
                        flag,
                        0.5,
                        -0.25
                    )
                )
                .unwrap()
                .0
                .to_bits(),
            expected.to_bits()
        );
        mixed.post_return(&mut store).unwrap();
        store.gc(None).unwrap();
    }
    let seen = instance
        .get_typed_func::<(), (f64,)>(&mut store, "seen")
        .unwrap();
    assert_eq!(seen.call(&mut store, ()).unwrap().0, 2.0);
    seen.post_return(&mut store).unwrap();
}

#[test]
fn exported_interfaces_keep_names_versions_shared_cells_and_source_effects() {
    let fragment = portable::prepare_fragment(
        "(def count 0) (def calc (fn [x] (do (set! count (+ count 1)) (+ x 1)))) (def seen (fn [] count))",
        &Environment::default(), Phase::Runtime,
    ).unwrap();
    let (resolve, selected) = world(
        "package test:interfaces@1.2.3; interface math { calc: func(x: u32) -> u32; } world api { export math; export renamed: math; export alias: interface { calc: func(x: s16) -> s16; } export empty: interface {} export seen: func() -> f64; }",
    );
    let bytes = portable::aot::component(
        &[fragment],
        &resolve,
        selected,
        &[
            (
                "test:interfaces/math@1.2.3#calc".into(),
                Symbol::new("calc"),
            ),
            ("alias#calc".into(), Symbol::new("calc")),
            ("renamed#calc".into(), Symbol::new("calc")),
            ("seen".into(), Symbol::new("seen")),
        ],
    )
    .unwrap();
    let mut imports = 0;
    for payload in wasmparser::Parser::new(0).parse_all(&bytes) {
        if let wasmparser::Payload::ComponentImportSection(section) = payload.unwrap() {
            imports += section.count();
        }
    }
    assert_eq!(imports, 0);
    let engine = engine();
    let component = Component::new(&engine, bytes).unwrap();
    let math = component
        .get_export_index(None, "test:interfaces/math@1.2.3")
        .unwrap();
    let math_calc = component.get_export_index(Some(&math), "calc").unwrap();
    let alias = component.get_export_index(None, "alias").unwrap();
    let alias_calc = component.get_export_index(Some(&alias), "calc").unwrap();
    let renamed = component.get_export_index(None, "renamed").unwrap();
    let renamed_calc = component.get_export_index(Some(&renamed), "calc").unwrap();
    assert!(component.get_export_index(None, "empty").is_some());
    assert!(component.get_export_index(None, "calc").is_none());
    for _ in 0..2 {
        let mut store = Store::new(&engine, ());
        store.set_fuel(20_000_000).unwrap();
        let instance = Linker::new(&engine)
            .instantiate(&mut store, &component)
            .unwrap();
        let math_calc = instance
            .get_typed_func::<(u32,), (u32,)>(&mut store, &math_calc)
            .unwrap();
        let alias_calc = instance
            .get_typed_func::<(i16,), (i16,)>(&mut store, &alias_calc)
            .unwrap();
        let renamed_calc = instance
            .get_typed_func::<(u32,), (u32,)>(&mut store, &renamed_calc)
            .unwrap();
        let seen = instance
            .get_typed_func::<(), (f64,)>(&mut store, "seen")
            .unwrap();
        assert_eq!(
            seen.call(&mut store, ()).unwrap().0.to_bits(),
            0.0_f64.to_bits()
        );
        seen.post_return(&mut store).unwrap();
        assert_eq!(
            math_calc.call(&mut store, (2147483647,)).unwrap().0,
            2147483648
        );
        math_calc.post_return(&mut store).unwrap();
        store.gc(None).unwrap();
        assert_eq!(alias_calc.call(&mut store, (-8,)).unwrap().0, -7);
        alias_calc.post_return(&mut store).unwrap();
        assert_eq!(renamed_calc.call(&mut store, (41,)).unwrap().0, 42);
        renamed_calc.post_return(&mut store).unwrap();
        assert_eq!(
            seen.call(&mut store, ()).unwrap().0.to_bits(),
            3.0_f64.to_bits()
        );
        seen.post_return(&mut store).unwrap();
    }
}

#[test]
fn exported_interface_mapping_paths_are_exact_and_types_remain_explicitly_unsupported() {
    let fragment = portable::prepare_fragment(
        "(def calc (fn [x] x))",
        &Environment::default(),
        Phase::Runtime,
    )
    .unwrap();
    let (resolve, selected) = world(
        "package test:mapping; world api { export api: interface { calc: func(x: f64) -> f64; } }",
    );
    for mapping in [
        vec![],
        vec![("calc".into(), Symbol::new("calc"))],
        vec![
            ("api#calc".into(), Symbol::new("calc")),
            ("api#calc".into(), Symbol::new("calc")),
        ],
    ] {
        let error = portable::aot::component(
            std::slice::from_ref(&fragment),
            &resolve,
            selected,
            &mapping,
        )
        .unwrap_err();
        assert_eq!(
            error.message,
            "WIT export api#calc needs exactly one explicit Suss var mapping"
        );
    }
    let error = portable::aot::component(
        std::slice::from_ref(&fragment),
        &resolve,
        selected,
        &[
            ("api#calc".into(), Symbol::new("calc")),
            ("calc".into(), Symbol::new("calc")),
        ],
    )
    .unwrap_err();
    assert_eq!(error.message, "Unknown WIT export mapping");
    let (resolve, selected) = world(
        "package test:types; interface api { type scalar = f64; calc: func(x: scalar) -> scalar; } world api-world { export api; }",
    );
    let error = portable::aot::component(
        &[fragment],
        &resolve,
        selected,
        &[("test:types/api#calc".into(), Symbol::new("calc"))],
    )
    .unwrap_err();
    assert_eq!(
        error.message,
        "Portable AOT interface type exports remain unimplemented"
    );
}
