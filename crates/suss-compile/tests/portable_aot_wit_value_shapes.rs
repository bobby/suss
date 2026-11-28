//! Execute recursive WIT schemas as native persistent source values.
#![cfg(not(target_family = "wasm"))]
use std::sync::OnceLock;
use suss_compile::Compiler;
use wasmtime::{
    Config, Engine, Store,
    component::{Component, Instance, Linker, Val},
};

fn compile(source: &str, exports: &str) -> Component {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    let engine = ENGINE.get_or_init(|| {
        let mut config = Config::new();
        config
            .wasm_gc(true)
            .gc_heap_initial_size(8 * 1024 * 1024)
            .wasm_function_references(true)
            .wasm_tail_call(true)
            .wasm_exceptions(true)
            .wasm_component_model(true)
            .wasm_component_model_map(true)
            .wasm_component_model_fixed_length_lists(true)
            .wasm_component_model_async(true)
            .consume_fuel(true)
            .cranelift_opt_level(wasmtime::OptLevel::None);
        Engine::new(&config).unwrap()
    });
    let wit = format!("package test:composite-shapes; world api {{ {exports} }}");
    let bytes = Compiler::new().compile(source, &wit).unwrap();
    let component = Component::new(engine, bytes).unwrap();
    assert_eq!(component.component_type().imports(engine).count(), 0);
    component
}

fn instantiate(source: &str, exports: &str) -> (Store<()>, Instance) {
    let component = compile(source, exports);
    let engine = component.engine();
    let mut store = Store::new(engine, ());
    store.set_fuel(100_000_000).unwrap();
    let instance = Linker::new(engine)
        .instantiate(&mut store, &component)
        .unwrap();
    store.set_fuel(100_000_000).unwrap();
    (store, instance)
}

#[test]
fn enum_keywords_flag_sets_and_general_variants_use_core_operations() {
    let names = (0..32)
        .map(|i| format!("f{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let declarations = format!(
        "enum state {{ waiting, active, done }} flags permissions {{ {names} }}
        variant event {{ idle, text(string), single(f32), wide(f64), choice(option<bool>) }}
        export transition: func(x: state) -> state;
        export permissions: func(x: permissions) -> permissions;
        export event: func(x: event) -> event;
        export events: func(xs: list<event>) -> list<event>;"
    );
    let (mut store, instance) = instantiate(
        "(defn ^:export transition [x] ({:waiting :active :active :done :done :waiting} x))
         (defn ^:export permissions [x] (conj (disj x :f1) :f31))
         (defn ^:export event [x] (if (= (first x) :text) (assoc x 1 (str (second x) \"!\")) x))
         (defn ^:export events [xs] (conj xs [:idle]))",
        &declarations,
    );
    let transition = instance.get_func(&mut store, "transition").unwrap();
    let mut output = [Val::Bool(false)];
    for (input, expected) in [
        ("waiting", "active"),
        ("active", "done"),
        ("done", "waiting"),
    ] {
        transition
            .call(&mut store, &[Val::Enum(input.into())], &mut output)
            .unwrap();
        assert_eq!(output[0], Val::Enum(expected.into()));
    }
    let permissions = instance.get_func(&mut store, "permissions").unwrap();
    for flags in [vec![], vec!["f0", "f1"], vec!["f31"], vec!["f1", "f31"]] {
        permissions
            .call(
                &mut store,
                &[Val::Flags(flags.iter().map(|s| (*s).into()).collect())],
                &mut output,
            )
            .unwrap();
        let mut expected = flags
            .into_iter()
            .filter(|s| *s != "f1" && *s != "f31")
            .map(String::from)
            .collect::<Vec<_>>();
        expected.push("f31".into());
        assert_eq!(output[0], Val::Flags(expected));
    }
    let event = instance.get_func(&mut store, "event").unwrap();
    let events = instance.get_func(&mut store, "events").unwrap();
    let values = vec![
        Val::Variant("idle".into(), None),
        Val::Variant("text".into(), Some(Box::new(Val::String("λ\0🦀".into())))),
        Val::Variant("single".into(), Some(Box::new(Val::Float32(1.5)))),
        Val::Variant("wide".into(), Some(Box::new(Val::Float64(f64::MAX)))),
        Val::Variant("choice".into(), Some(Box::new(Val::Option(None)))),
        Val::Variant(
            "choice".into(),
            Some(Box::new(Val::Option(Some(Box::new(Val::Bool(false)))))),
        ),
    ];
    for value in &values {
        event
            .call(&mut store, &[value.clone()], &mut output)
            .unwrap();
        let expected = if let Val::Variant(name, _) = value {
            if name == "text" {
                Val::Variant("text".into(), Some(Box::new(Val::String("λ\0🦀!".into()))))
            } else {
                value.clone()
            }
        } else {
            unreachable!()
        };
        assert_eq!(output[0], expected);
        store.gc(None).unwrap();
    }
    events
        .call(&mut store, &[Val::List(values.clone())], &mut output)
        .unwrap();
    let mut expected = values;
    expected.push(Val::Variant("idle".into(), None));
    assert_eq!(output[0], Val::List(expected));
}

#[test]
fn narrow_flags_keep_declared_bit_positions_in_records() {
    let (mut store, instance) = instantiate(
        "(defn ^:export narrow [x] (assoc x :one (conj (:one x) :only) :nine (conj (:nine x) :f8)))",
        "flags one { only } flags nine { f0, f1, f2, f3, f4, f5, f6, f7, f8 }
         record row { one: one, nine: nine }
         export narrow: func(x: row) -> row;",
    );
    let narrow = instance.get_func(&mut store, "narrow").unwrap();
    let mut output = [Val::Bool(false)];
    narrow
        .call(
            &mut store,
            &[Val::Record(vec![
                ("one".into(), Val::Flags(vec![])),
                ("nine".into(), Val::Flags(vec!["f0".into()])),
            ])],
            &mut output,
        )
        .unwrap();
    assert_eq!(
        output[0],
        Val::Record(vec![
            ("one".into(), Val::Flags(vec!["only".into()])),
            ("nine".into(), Val::Flags(vec!["f0".into(), "f8".into()]))
        ])
    );
}
#[test]
fn wit_maps_use_native_assoc_conj_and_last_duplicate_wins() {
    let (mut store, instance) = instantiate(
        "(defn ^:export amend [xs] (update (conj xs [\"added\" [-9223372036854775808]]) \"same\" conj 9223372036854775807))",
        "type table = map<string, list<s64>>; export amend: func(xs: table) -> table;",
    );
    let amend = instance.get_func(&mut store, "amend").unwrap();
    let pair = |key: &str, values: &[i64]| {
        (
            Val::String(key.into()),
            Val::List(values.iter().copied().map(Val::S64).collect()),
        )
    };
    for _ in 0..20 {
        let mut output = [Val::Bool(false)];
        amend
            .call(
                &mut store,
                &[Val::Map(vec![
                    pair("same", &[1]),
                    pair("λ\0🦀", &[9007199254740993]),
                    pair("same", &[2]),
                ])],
                &mut output,
            )
            .unwrap();
        let Val::Map(actual) = &output[0] else {
            panic!("wrong result {output:?}")
        };
        assert_eq!(actual.len(), 3);
        for expected in [
            pair("same", &[2, i64::MAX]),
            pair("λ\0🦀", &[9007199254740993]),
            pair("added", &[i64::MIN]),
        ] {
            assert!(
                actual.contains(&expected),
                "missing {expected:?} in {actual:?}"
            );
        }
        store.gc(None).unwrap();
    }
}

#[test]
fn fixed_length_lists_are_native_vectors_with_nested_owned_payloads() {
    let (mut store, instance) = instantiate(
        "(defn ^:export rotate [xs] (conj (subvec xs 1) (first xs)))",
        "type row = list<tuple<s64, string>, 3>; export rotate: func(xs: row) -> row;",
    );
    let rotate = instance.get_func(&mut store, "rotate").unwrap();
    let pair = |n, s: &str| Val::Tuple(vec![Val::S64(n), Val::String(s.into())]);
    let values = vec![
        pair(i64::MIN, "left"),
        pair(9007199254740993, "λ\0🦀"),
        pair(i64::MAX, "right"),
    ];
    let mut expected = values.clone();
    expected.rotate_left(1);
    for _ in 0..20 {
        let mut output = [Val::Bool(false)];
        rotate
            .call(
                &mut store,
                &[Val::FixedLengthList(values.clone())],
                &mut output,
            )
            .unwrap();
        assert_eq!(output[0], Val::FixedLengthList(expected.clone()));
        store.gc(None).unwrap();
    }
}

#[test]
fn nested_numeric_packed_arrays_preserve_unsigned_bits_and_exact_values() {
    let (mut store, instance) = instantiate(
        "(defn ^:export packed []
            [(byte-array [-1 0 127 -128])
             (long-array [-9223372036854775808 9007199254740993 9223372036854775807])
             (double-array [-0.0 1.5])])
         (defn ^:export nested [] [(byte-array [-1 -128]) (vector-of :byte -1 1)])",
        "type row = tuple<list<u8>, list<s64>, list<f64>>;
         export packed: func() -> row;
         export nested: func() -> list<list<u8>>;",
    );
    type Row = (Vec<u8>, Vec<i64>, Vec<f64>);
    let packed = instance
        .get_typed_func::<(), (Row,)>(&mut store, "packed")
        .unwrap();
    let nested = instance
        .get_typed_func::<(), (Vec<Vec<u8>>,)>(&mut store, "nested")
        .unwrap();
    for _ in 0..20 {
        let (bytes, longs, doubles) = packed.call(&mut store, ()).unwrap().0;
        assert_eq!(bytes, [255, 0, 127, 128]);
        assert_eq!(longs, [i64::MIN, 9007199254740993, i64::MAX]);
        assert_eq!(doubles[0].to_bits(), (-0.0_f64).to_bits());
        assert_eq!(doubles[1], 1.5);
        assert_eq!(
            nested.call(&mut store, ()).unwrap().0,
            [vec![255, 128], vec![255, 1]]
        );
        store.gc(None).unwrap();
    }
}

#[derive(Default)]
struct Limits {
    canonical_peak: usize,
}
impl wasmtime::ResourceLimiter for Limits {
    fn memory_growing(
        &mut self,
        current: usize,
        desired: usize,
        _: Option<usize>,
    ) -> wasmtime::Result<bool> {
        if current >= 17 * 65536 || desired == 17 * 65536 || desired == 8 * 1024 * 1024 {
            Ok(desired <= 64 * 1024 * 1024)
        } else {
            self.canonical_peak = self.canonical_peak.max(desired);
            Ok(desired <= 65536)
        }
    }
    fn table_growing(&mut self, _: usize, _: usize, _: Option<usize>) -> wasmtime::Result<bool> {
        Ok(true)
    }
}

fn language_message(store: &mut Store<Limits>) -> String {
    use wasmtime::AsContextMut;
    let exception = store
        .as_context_mut()
        .take_pending_exception()
        .expect("expected a pending language boundary exception");
    let fields = exception.fields(&mut *store).unwrap().collect::<Vec<_>>();
    assert_eq!(fields.len(), 1);
    let payload = fields[0].unwrap_anyref().unwrap();
    let array = if let Some(array) = payload.as_array(&*store).unwrap() {
        array
    } else {
        payload
            .as_struct(&*store)
            .unwrap()
            .unwrap()
            .field(&mut *store, 1)
            .unwrap()
            .unwrap_anyref()
            .unwrap()
            .as_array(&*store)
            .unwrap()
            .unwrap()
    };
    let units = array
        .elems(&mut *store)
        .unwrap()
        .map(|unit| unit.unwrap_i32() as u16)
        .collect::<Vec<_>>();
    String::from_utf16(&units).unwrap()
}

#[test]
fn malformed_composite_results_raise_language_errors_before_canonical_growth() {
    let source =
        "(def large (loop [n 16 text \"x\"] (if (zero? n) text (recur (dec n) (str text text)))))
      (defn ^:export bad-enum [] :unknown)
      (defn ^:export bad-enum-type [] \"off\")
      (defn ^:export bad-category [] [:payload [1.0 \"text\"]])
      (defn ^:export bad-tuple [] [large])
      (defn ^:export bad-objects [] [large (object-array [1])])
      (defn ^:export bad-tag [] [:unknown])
      (defn ^:export bad-flags [] #{:write :unknown})
      (defn ^:export bad-variant [] [:payload [256 \"text\"]])
      (defn ^:export bad-arity [] [:idle 0])
      (defn ^:export bad-record [] {:a large :b [:none] :extra 0})
      (defn ^:export bad-missing [] {:a large})
      (defn ^:export bad-array [] [large (int-array [1])])
      (defn ^:export bad-fixed [] [1 2])
      (defn ^:export bad-utf [] [large \"\\uD800\"])
      (defn ^:export bad-map [] [[\"key\" 1]])";
    let declarations = "enum state { off, on } flags permissions { read, write }
      variant event { idle, payload(tuple<u8, string>) }
      record row { a: string, b: option<list<u8>> }
      type fixed = list<u8, 3>;
      export bad-enum: func() -> state; export bad-enum-type: func() -> state;
      export bad-category: func() -> event; export bad-tag: func() -> event;
      export bad-tuple: func() -> tuple<string, u8>;
      export bad-objects: func() -> tuple<string, list<u8>>; export bad-flags: func() -> permissions;
      export bad-variant: func() -> event; export bad-arity: func() -> event;
      export bad-record: func() -> row; export bad-missing: func() -> row;
      export bad-array: func() -> tuple<string, list<u8>>;
      export bad-fixed: func() -> fixed;
      export bad-utf: func() -> tuple<string, string>;
      export bad-map: func() -> map<string, u8>;";
    let component = compile(source, declarations);
    for (name, diagnostic) in [
        ("bad-enum", "state"),
        ("bad-enum-type", "state"),
        ("bad-category", "scalar"),
        ("bad-tag", "event"),
        ("bad-tuple", "tuple"),
        ("bad-objects", "list"),
        ("bad-flags", "permissions"),
        ("bad-variant", "scalar"),
        ("bad-arity", "event"),
        ("bad-record", "row"),
        ("bad-missing", "row"),
        ("bad-array", "primitive array"),
        ("bad-fixed", "fixed"),
        ("bad-utf", "Unicode"),
        ("bad-map", "map"),
    ] {
        let mut store = Store::new(component.engine(), Limits::default());
        store.limiter(|limits| limits);
        store.set_fuel(100_000_000).unwrap();
        let instance = Linker::new(component.engine())
            .instantiate(&mut store, &component)
            .unwrap();
        store.set_fuel(100_000_000).unwrap();
        instance
            .get_func(&mut store, name)
            .unwrap()
            .call(&mut store, &[], &mut [Val::Bool(false)])
            .unwrap_err();
        let message = language_message(&mut store);
        assert!(
            message.contains("WIT") && message.contains(diagnostic),
            "{name}: {message}"
        );
        assert_eq!(
            store.data().canonical_peak,
            65536,
            "{name}: canonical allocation preceded full validation"
        );
    }
}

#[test]
fn ten_thousand_nested_composite_calls_reuse_one_transfer_page_and_retain_owned_values() {
    let component = compile(
        "(def saved (atom nil)) (defn ^:export echo [x] (reset! saved x) x) (defn ^:export recall [] @saved)",
        "type row = tuple<list<tuple<u64, string>>, option<option<bool>>>;
         export echo: func(x: row) -> row; export recall: func() -> row;",
    );
    let mut store = Store::new(component.engine(), Limits::default());
    store.limiter(|limits| limits);
    store.set_fuel(100_000_000).unwrap();
    let instance = Linker::new(component.engine())
        .instantiate(&mut store, &component)
        .unwrap();
    let echo = instance.get_func(&mut store, "echo").unwrap();
    let recall = instance.get_func(&mut store, "recall").unwrap();
    let input = Val::Tuple(vec![
        Val::List(vec![Val::Tuple(vec![
            Val::U64(u64::MAX),
            Val::String("λ\0🦀".into()),
        ])]),
        Val::Option(Some(Box::new(Val::Option(Some(Box::new(Val::Bool(
            false,
        ))))))),
    ]);
    let mut output = [Val::Bool(false)];
    for i in 0..10000 {
        if i % 100 == 0 {
            store.set_fuel(100_000_000).unwrap();
            store.gc(None).unwrap();
        }
        echo.call(&mut store, &[input.clone()], &mut output)
            .unwrap();
        assert_eq!(output[0], input);
        if i % 500 == 0 {
            store.gc(None).unwrap();
            recall.call(&mut store, &[], &mut output).unwrap();
            assert_eq!(output[0], input);
        }
    }
    assert_eq!(store.data().canonical_peak, 65536);
}

#[test]
fn composite_schemas_retain_nominal_types_and_captured_core_functions() {
    let source = "(ns suss.core)
      (def count (fn [x] 0)) (def nth (fn [x i] 99))
      (def map? (fn [x] true)) (def set? (fn [x] true))
      (def get (fn [x k] nil)) (def assoc (fn [& xs] nil))
      (def contains? (fn [x k] false)) (def = (fn [x y] true))
      (def PersistentVector nil) (def Subvec nil) (def Keyword nil)
      (ns user) (defn ^:export echo [x] x) (defn ^:export bad [] {})";
    let declarations = "enum state { off, on } flags permissions { read, write }
      record row { state: state, permissions: permissions, values: list<s64>, table: map<string, u8> }
      export echo: func(x: row) -> row; export bad: func() -> row;";
    let (mut store, instance) = instantiate(source, declarations);
    let echo = instance.get_func(&mut store, "echo").unwrap();
    let mut output = [Val::Bool(false)];
    let expected = Val::Record(vec![
        ("state".into(), Val::Enum("on".into())),
        ("permissions".into(), Val::Flags(vec!["read".into()])),
        (
            "values".into(),
            Val::List(vec![Val::S64(9007199254740993), Val::S64(i64::MIN)]),
        ),
        (
            "table".into(),
            Val::Map(vec![(Val::String("key".into()), Val::U8(255))]),
        ),
    ]);
    echo.call(&mut store, &[expected.clone()], &mut output)
        .unwrap();
    assert_eq!(output[0], expected);
    store.gc(None).unwrap();
    instance
        .get_func(&mut store, "bad")
        .unwrap()
        .call(&mut store, &[], &mut output)
        .unwrap_err();
    use wasmtime::AsContextMut;
    assert!(store.as_context_mut().take_pending_exception().is_some());
}

#[test]
fn invalid_canonical_nominal_and_fixed_types_fail_before_source_macros() {
    let flags = (0..33)
        .map(|i| format!("f{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    for (definition, ty, expected) in [
        ("enum empty {}".to_owned(), "empty", "enum"),
        ("flags empty {}".to_owned(), "empty", "flags"),
        (format!("flags excessive {{ {flags} }}"), "excessive", "32"),
        ("type empty = list<u8, 0>;".to_owned(), "empty", "element"),
    ] {
        let wit = format!(
            "package test:invalid-shape; world api {{ {definition} export value: func() -> {ty}; }}"
        );
        let error = Compiler::new().compile("(defmacro effect [] (throw \"source macro ran\")) (defn ^:export value [] (effect))", &wit).unwrap_err().to_string();
        assert!(error.contains(expected), "{error}");
        assert!(!error.contains("source macro ran"), "{error}");
    }
}
