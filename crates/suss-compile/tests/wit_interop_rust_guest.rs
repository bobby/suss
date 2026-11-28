//! Execute a pinned generated Rust guest through the real Suss component boundary.
#![cfg(not(target_family = "wasm"))]
use wasm_encoder::{
    Alias, ComponentAliasSection, ComponentExportKind as Kind, ComponentExportSection,
    ComponentInstanceSection, ComponentSectionId, RawSection,
};
use wasmtime::{
    Config, Engine, Store,
    component::{Component, Linker, Val},
};
fn normalize(value: Val) -> Val {
    match value {
        Val::Map(entries) => {
            let mut entries: Vec<_> = entries
                .into_iter()
                .map(|(k, v)| (normalize(k), normalize(v)))
                .collect();
            entries.sort_by_key(|(k, _)| format!("{k:?}"));
            Val::Map(entries)
        }
        Val::Record(entries) => Val::Record(
            entries
                .into_iter()
                .map(|(k, v)| (k, normalize(v)))
                .collect(),
        ),
        Val::List(v) => Val::List(v.into_iter().map(normalize).collect()),
        Val::FixedLengthList(v) => Val::FixedLengthList(v.into_iter().map(normalize).collect()),
        Val::Tuple(v) => Val::Tuple(v.into_iter().map(normalize).collect()),
        Val::Option(v) => Val::Option(v.map(|v| Box::new(normalize(*v)))),
        Val::Result(Ok(v)) => Val::Result(Ok(v.map(|v| Box::new(normalize(*v))))),
        Val::Result(Err(v)) => Val::Result(Err(v.map(|v| Box::new(normalize(*v))))),
        Val::Variant(k, v) => Val::Variant(k, v.map(|v| Box::new(normalize(*v)))),
        v => v,
    }
}
fn record(fields: Vec<(&str, Val)>) -> Val {
    Val::Record(fields.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
fn packet(number: usize, option: usize, variant: bool) -> Val {
    let large = [u64::MAX, 9007199254740993, 0][number];
    let signed = [i64::MIN, 9007199254740993, i64::MAX][number];
    let small = [-0.0, f32::MAX, f32::NEG_INFINITY][number];
    let wide = [f64::MAX, -0.0, f64::INFINITY][number];
    record(vec![
        (
            "numbers",
            record(vec![
                ("truth", Val::Bool(number % 2 == 0)),
                ("tiny-s", Val::S8([i8::MIN, i8::MAX, 0][number])),
                ("tiny-u", Val::U8([0, u8::MAX, 1][number])),
                ("short-s", Val::S16([i16::MIN, i16::MAX, 0][number])),
                ("short-u", Val::U16([0, u16::MAX, 1][number])),
                ("int-s", Val::S32([i32::MIN, i32::MAX, 0][number])),
                ("int-u", Val::U32([0, u32::MAX, 1][number])),
                ("long-s", Val::S64(signed)),
                ("long-u", Val::U64(large)),
                ("small", Val::Float32(small)),
                ("wide", Val::Float64(wide)),
                ("scalar", Val::Char('🦀')),
                ("text", Val::String("left λ\0🦀".into())),
            ]),
        ),
        ("rights", Val::Flags(vec!["read".into(), "execute".into()])),
        (
            "category",
            Val::Enum(["first", "middle", "last"][number].into()),
        ),
        (
            "selected",
            if variant {
                Val::Variant(
                    "data".into(),
                    Some(Box::new(Val::Tuple(vec![
                        Val::String("selected λ🦀".into()),
                        Val::List(vec![Val::U8(0), Val::U8(255)]),
                    ]))),
                )
            } else {
                Val::Variant("empty".into(), None)
            },
        ),
        (
            "nested",
            Val::List(vec![
                Val::Option(None),
                Val::Option(Some(Box::new(Val::Result(Ok(Some(Box::new(Val::Tuple(
                    vec![Val::U64(large), Val::String("ok 🦀".into())],
                )))))))),
                Val::Option(Some(Box::new(Val::Result(Err(Some(Box::new(
                    Val::String("typed error λ".into()),
                ))))))),
            ]),
        ),
        (
            "lookup",
            Val::Map(vec![
                (
                    Val::String("λ🦀".into()),
                    Val::List(vec![
                        Val::Option(None),
                        Val::Option(Some(Box::new(Val::U64(large)))),
                    ]),
                ),
                (Val::String("".into()), Val::List(vec![])),
            ]),
        ),
        (
            "fixed",
            Val::FixedLengthList(vec![Val::U16(0), Val::U16(255), Val::U16(u16::MAX)]),
        ),
        (
            "maybe",
            match option {
                0 => Val::Option(None),
                1 => Val::Option(Some(Box::new(Val::Option(None)))),
                2 => Val::Option(Some(Box::new(Val::Option(Some(Box::new(Val::Bool(
                    false,
                ))))))),
                _ => Val::Option(Some(Box::new(Val::Option(Some(Box::new(Val::Bool(true))))))),
            },
        ),
    ])
}
fn check_bits(actual: &Val, expected: &Val) {
    match (actual, expected) {
        (Val::Float32(a), Val::Float32(e)) => assert_eq!(a.to_bits(), e.to_bits()),
        (Val::Float64(a), Val::Float64(e)) => assert_eq!(a.to_bits(), e.to_bits()),
        (Val::Record(a), Val::Record(e)) => {
            for ((ak, av), (ek, ev)) in a.iter().zip(e) {
                assert_eq!(ak, ek);
                check_bits(av, ev)
            }
        }
        _ => {}
    }
}
fn sha(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}
#[test]
fn generated_rust_guest_and_suss_preserve_boundary_values_and_reject_before_host_effects() {
    use suss_compile::portable_aot::{AotOptions, compile_file_with_options};
    use suss_reader::Symbol;
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/wit-interop-rust");
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
    for (name, expected) in manifest["source_hashes"].as_object().unwrap() {
        assert_eq!(
            sha(&std::fs::read(root.join(name)).unwrap()),
            expected.as_str().unwrap(),
            "fixture source {name} changed without regenerating its Rust guest"
        );
    }
    let checked = std::fs::read(root.join("provider.wasm")).unwrap();
    assert_eq!(
        sha(&checked),
        manifest["component_sha256"].as_str().unwrap()
    );
    let rust = std::env::var_os("SUSS_WIT_RUST_GUEST_COMPONENT")
        .map(|path| std::fs::read(path).unwrap())
        .unwrap_or(checked);
    let suss = compile_file_with_options(
        &root.join("app.sus"),
        &root.join("api.wit"),
        Some("suss-consumer"),
        &[],
        &AotOptions {
            imports: [
                (
                    "fixture:rust-interop/provider@0.1.0#echo",
                    "fixture.host/echo",
                ),
                (
                    "fixture:rust-interop/provider@0.1.0#calls",
                    "fixture.host/calls",
                ),
            ]
            .into_iter()
            .map(|(name, source)| (name.into(), Symbol::parse(source)))
            .collect(),
            exports: ["echo", "seen", "rejected"]
                .into_iter()
                .map(|name| (name.into(), Symbol::namespaced("fixture.app", name)))
                .collect(),
            ..Default::default()
        },
    )
    .unwrap();
    let mut outer = wasm_encoder::Component::new();
    for data in [&rust, &suss] {
        outer.section(&RawSection {
            id: ComponentSectionId::Component as u8,
            data,
        });
    }
    let mut instances = ComponentInstanceSection::new();
    instances.instantiate(0, [] as [(&str, Kind, u32); 0]);
    outer.section(&instances);
    let mut aliases = ComponentAliasSection::new();
    aliases.alias(Alias::InstanceExport {
        instance: 0,
        kind: Kind::Instance,
        name: "fixture:rust-interop/provider@0.1.0",
    });
    outer.section(&aliases);
    let mut instances = ComponentInstanceSection::new();
    instances.instantiate(
        1,
        [("fixture:rust-interop/provider@0.1.0", Kind::Instance, 1)],
    );
    outer.section(&instances);
    let mut exports = ComponentExportSection::new();
    exports.export("rust", Kind::Instance, 0, None);
    exports.export("suss", Kind::Instance, 2, None);
    outer.section(&exports);
    let bytes = outer.finish();
    wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::all())
        .validate_all(&bytes)
        .unwrap();
    let mut config = Config::new();
    config
        .wasm_gc(true)
        .gc_heap_initial_size(8 * 1024 * 1024)
        .wasm_tail_call(true)
        .wasm_exceptions(true)
        .wasm_component_model(true)
        .wasm_component_model_map(true)
        .wasm_component_model_fixed_length_lists(true)
        .consume_fuel(true)
        .cranelift_opt_level(wasmtime::OptLevel::None);
    let engine = Engine::new(&config).unwrap();
    let component = Component::new(&engine, &bytes).unwrap();
    assert_eq!(component.component_type().imports(&engine).count(), 0);
    let mut store = Store::new(&engine, ());
    store.set_fuel(100_000_000).unwrap();
    let instance = Linker::new(&engine)
        .instantiate(&mut store, &component)
        .unwrap();
    let rust = instance.get_export_index(&mut store, None, "rust").unwrap();
    let provider = instance
        .get_export_index(
            &mut store,
            Some(&rust),
            "fixture:rust-interop/provider@0.1.0",
        )
        .unwrap();
    let direct = instance
        .get_export_index(&mut store, Some(&provider), "echo")
        .unwrap();
    let suss = instance.get_export_index(&mut store, None, "suss").unwrap();
    let through = instance
        .get_export_index(&mut store, Some(&suss), "echo")
        .unwrap();
    let mut count = 0;
    for number in 0..3 {
        for option in 0..4 {
            for variant in [false, true] {
                let expected = packet(number, option, variant);
                for route in [direct, through] {
                    store.set_fuel(100_000_000).unwrap();
                    let function = instance.get_func(&mut store, route).unwrap();
                    let mut results = [Val::Bool(false)];
                    function
                        .call(&mut store, &[expected.clone()], &mut results)
                        .unwrap();
                    check_bits(&results[0], &expected);
                    assert_eq!(normalize(results[0].clone()), normalize(expected.clone()));
                    store.gc(None).unwrap();
                    count += 1;
                }
            }
        }
    }
    assert_eq!(count, 48);
    let seen = instance
        .get_export_index(&mut store, Some(&suss), "seen")
        .unwrap();
    let seen = instance
        .get_typed_func::<(), (u32,)>(&mut store, seen)
        .unwrap();
    assert_eq!(seen.call(&mut store, ()).unwrap().0, 48);
    let rejected = instance
        .get_export_index(&mut store, Some(&suss), "rejected")
        .unwrap();
    let rejected = instance
        .get_typed_func::<(), (bool,)>(&mut store, rejected)
        .unwrap();
    assert!(rejected.call(&mut store, ()).unwrap().0);
    assert_eq!(
        seen.call(&mut store, ()).unwrap().0,
        48,
        "invalid flags reached Rust guest"
    );
    println!(
        "PASS48direct-Rust and Rust-host -> Suss -> generated-Rust-guest calls; malformed source flags reject before Rust effects; no outer imports"
    );
}
