//! Execute public WIT aliases through their original resolved type identities.
#![cfg(not(target_family = "wasm"))]

use std::sync::OnceLock;
use suss_compile::portable::{
    self,
    resolve::{Environment, Phase},
};
use suss_reader::Symbol;
#[test]
fn exported_type_aliases_preserve_named_identity_and_exact_values() {
    let mut resolve = wit_parser::Resolve::new();
    let package = resolve.push_str("api.wit", "package test:named@1.2.3; interface values { type id = u64; type letter = char; echo: func(x: id) -> id; same: func(x: letter) -> letter; } world api { export values; }").unwrap();
    let world = resolve.select_world(&[package], None).unwrap();
    let fragment = portable::prepare_fragment(
        "(def echo (fn [x] x))",
        &Environment::default(),
        Phase::Runtime,
    )
    .unwrap();
    let bytes = portable::aot::component(
        &[fragment],
        &resolve,
        world,
        &[
            ("test:named/values@1.2.3#echo".into(), Symbol::new("echo")),
            ("test:named/values@1.2.3#same".into(), Symbol::new("echo")),
        ],
    )
    .unwrap();
    let engine = engine();
    let component = wasmtime::component::Component::new(&engine, bytes).unwrap();
    let interface = component
        .get_export_index(None, "test:named/values@1.2.3")
        .unwrap();
    assert!(component.get_export_index(Some(&interface), "id").is_some());
    assert!(
        component
            .get_export_index(Some(&interface), "letter")
            .is_some()
    );
    let echo = component
        .get_export_index(Some(&interface), "echo")
        .unwrap();
    let same = component
        .get_export_index(Some(&interface), "same")
        .unwrap();
    let mut store = wasmtime::Store::new(&engine, ());
    store.set_fuel(100_000_000).unwrap();
    let instance = wasmtime::component::Linker::new(&engine)
        .instantiate(&mut store, &component)
        .unwrap();
    assert_eq!(
        instance
            .get_typed_func::<(u64,), (u64,)>(&mut store, &echo)
            .unwrap()
            .call(&mut store, (u64::MAX,))
            .unwrap(),
        (u64::MAX,)
    );
    assert_eq!(
        instance
            .get_typed_func::<(char,), (char,)>(&mut store, &same)
            .unwrap()
            .call(&mut store, ('🦀',))
            .unwrap(),
        ('🦀',)
    );
}

fn engine() -> wasmtime::Engine {
    static ENGINE: OnceLock<wasmtime::Engine> = OnceLock::new();
    ENGINE
        .get_or_init(|| {
            let mut config = wasmtime::Config::new();
            config
                .wasm_gc(true)
                .wasm_function_references(true)
                .wasm_tail_call(true)
                .wasm_exceptions(true)
                .wasm_component_model(true)
                .wasm_component_model_implements(true)
                .consume_fuel(true)
                .cranelift_opt_level(wasmtime::OptLevel::None);
            wasmtime::Engine::new(&config).unwrap()
        })
        .clone()
}

#[test]
fn named_list_option_and_alias_interfaces_keep_types_and_native_core_operations() {
    let mut resolve = wit_parser::Resolve::new();
    let package = resolve
        .push_str(
            "api.wit",
            "package test:named-values@1.2.3;
        interface values { type texts = list<string>; type maybe = option<char>;
          append: func(xs: texts, x: string) -> texts; same: func(x: maybe) -> maybe;
        } world api { export values; export renamed: values; }",
        )
        .unwrap();
    let world = resolve.select_world(&[package], None).unwrap();
    let fragments = suss_compile::portable_aot::prepare_source(
        "(defn append [xs x] (conj xs x)) (defn same [x] x)",
        None,
        &[],
    )
    .unwrap();
    let mappings = ["test:named-values/values@1.2.3", "renamed"]
        .into_iter()
        .flat_map(|interface| {
            [
                (format!("{interface}#append"), Symbol::new("append")),
                (format!("{interface}#same"), Symbol::new("same")),
            ]
        })
        .collect::<Vec<_>>();
    let bytes = portable::aot::component(&fragments, &resolve, world, &mappings).unwrap();
    let engine = engine();
    let component = wasmtime::component::Component::new(&engine, &bytes).unwrap();
    assert_eq!(component.component_type().imports(&engine).count(), 0);
    let mut store = wasmtime::Store::new(&engine, ());
    store.set_fuel(100_000_000).unwrap();
    let instance = wasmtime::component::Linker::new(&engine)
        .instantiate(&mut store, &component)
        .unwrap();
    for name in ["test:named-values/values@1.2.3", "renamed"] {
        let interface = component.get_export_index(None, name).unwrap();
        for ty in ["texts", "maybe"] {
            assert!(component.get_export_index(Some(&interface), ty).is_some());
        }
        let append = component
            .get_export_index(Some(&interface), "append")
            .unwrap();
        let append = instance
            .get_typed_func::<(Vec<String>, String), (Vec<String>,)>(&mut store, &append)
            .unwrap();
        assert_eq!(
            append
                .call(&mut store, (vec!["left λ".into()], "🦀".into()))
                .unwrap(),
            (vec!["left λ".into(), "🦀".into()],)
        );
        let same = component
            .get_export_index(Some(&interface), "same")
            .unwrap();
        let same = instance
            .get_typed_func::<(Option<char>,), (Option<char>,)>(&mut store, &same)
            .unwrap();
        for value in [None, Some('🦀')] {
            assert_eq!(same.call(&mut store, (value,)).unwrap(), (value,));
        }
        store.gc(None).unwrap();
    }
}
