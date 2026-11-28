//! Canonical `char` is a Unicode scalar; language strings remain UTF-16.
#![cfg(not(target_family = "wasm"))]

use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};
use suss_compile::Compiler;
use wasmtime::{
    AsContextMut, Config, Engine, Store,
    component::{Component, Instance, Linker, Val},
};

fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| {
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
}

fn instantiate(source: &str, exports: &str) -> (Store<()>, Instance) {
    static COMPONENTS: OnceLock<Mutex<HashMap<String, Component>>> = OnceLock::new();
    let key = format!("{source}\0{exports}");
    let cached = COMPONENTS
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .get(&key)
        .cloned();
    let component = cached.unwrap_or_else(|| {
        let wit = format!("package test:characters; world api {{ {exports} }}");
        let bytes = Compiler::new().compile(source, &wit).unwrap();
        let component = Component::new(engine(), bytes).unwrap();
        assert_eq!(component.component_type().imports(engine()).count(), 0);
        COMPONENTS
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .insert(key, component.clone());
        component
    });
    let mut store = Store::new(engine(), ());
    store.set_fuel(100_000_000).unwrap();
    let instance = Linker::new(engine())
        .instantiate(&mut store, &component)
        .unwrap();
    store.set_fuel(100_000_000).unwrap();
    (store, instance)
}

#[test]
fn wit_char_round_trips_unicode_scalars_and_retains_utf16_across_gc() {
    let (mut store, instance) = instantiate(
        "(def saved (atom nil))
         (defn ^:export same [x] (reset! saved x) x)
         (defn ^:export recall [] @saved)
         (defn ^:export units [] (count @saved))",
        "export same: func(x: char) -> char;
         export recall: func() -> char;
         export units: func() -> s64;",
    );
    let same = instance
        .get_typed_func::<(char,), (char,)>(&mut store, "same")
        .unwrap();
    let recall = instance
        .get_typed_func::<(), (char,)>(&mut store, "recall")
        .unwrap();
    let units = instance
        .get_typed_func::<(), (i64,)>(&mut store, "units")
        .unwrap();
    for value in [
        '\0',
        'A',
        '\u{d7ff}',
        '\u{e000}',
        '\u{ffff}',
        '\u{10000}',
        '🦀',
        '\u{10ffff}',
    ] {
        assert_eq!(same.call(&mut store, (value,)).unwrap(), (value,));
        same.post_return(&mut store).unwrap();
        store.gc(None).unwrap();
        assert_eq!(recall.call(&mut store, ()).unwrap(), (value,));
        recall.post_return(&mut store).unwrap();
        assert_eq!(
            units.call(&mut store, ()).unwrap(),
            (value.len_utf16() as i64,)
        );
        units.post_return(&mut store).unwrap();
    }
}

#[test]
fn wit_char_lists_and_options_preserve_scalar_boundaries() {
    let (mut store, instance) = instantiate(
        "(defn ^:export chars [xs] xs) (defn ^:export optional [x] x)
         (defn ^:export append [xs x] (conj xs x))
         (defn ^:export summary [xs] [(count xs) (reduce + (map count xs))])",
        "export chars: func(xs: list<char>) -> list<char>;
         export optional: func(x: option<char>) -> option<char>;
         export append: func(xs: list<char>, x: char) -> list<char>;
         export summary: func(xs: list<char>) -> list<s64>;",
    );
    let chars = instance
        .get_typed_func::<(Vec<char>,), (Vec<char>,)>(&mut store, "chars")
        .unwrap();
    let input = vec!['\0', 'é', '🦀', '\u{10ffff}'];
    assert_eq!(
        chars.call(&mut store, (input.clone(),)).unwrap(),
        (input.clone(),)
    );
    chars.post_return(&mut store).unwrap();
    let append = instance
        .get_typed_func::<(Vec<char>, char), (Vec<char>,)>(&mut store, "append")
        .unwrap();
    let mut expected = input.clone();
    expected.push('λ');
    assert_eq!(
        append.call(&mut store, (input.clone(), 'λ')).unwrap(),
        (expected,)
    );
    append.post_return(&mut store).unwrap();
    let summary = instance
        .get_typed_func::<(Vec<char>,), (Vec<i64>,)>(&mut store, "summary")
        .unwrap();
    assert_eq!(summary.call(&mut store, (input,)).unwrap(), (vec![4, 6],));
    summary.post_return(&mut store).unwrap();
    let optional = instance
        .get_typed_func::<(Option<char>,), (Option<char>,)>(&mut store, "optional")
        .unwrap();
    for value in [None, Some('\0'), Some('🦀')] {
        assert_eq!(optional.call(&mut store, (value,)).unwrap(), (value,));
        optional.post_return(&mut store).unwrap();
    }
}

#[test]
fn wit_char_rejects_nonstrings_empty_multiple_and_unpaired_units() {
    let expressions = [
        "nil",
        "65",
        "true",
        "\"\"",
        "\"ab\"",
        "\"🦀x\"",
        "\"\\uD800\"",
        "\"\\uDC00\"",
        "\"\\uD800A\"",
    ];
    let mut source = String::new();
    let mut exports = String::new();
    for (i, expression) in expressions.iter().enumerate() {
        source.push_str(&format!("(defn ^:export bad-{i} [] {expression}) "));
        exports.push_str(&format!("export bad-{i}: func() -> char; "));
    }
    for (i, expression) in expressions.iter().enumerate() {
        let (mut store, instance) = instantiate(&source, &exports);
        let bad = instance.get_func(&mut store, &format!("bad-{i}")).unwrap();
        bad.call(&mut store, &[], &mut [Val::Char(' ')])
            .unwrap_err();
        // Canonical calls report an uncaught exception as a component trap;
        // its actual pending language payload distinguishes it from a cast trap.
        let exception = store
            .as_context_mut()
            .take_pending_exception()
            .unwrap_or_else(|| panic!("missing language exception for {expression}"));
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
            format!("WIT export bad-{i} returned an incompatible Unicode scalar")
        );
    }
}
