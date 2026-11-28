//! Shared executing fixture for public string/file compilation of nested maps.
use wasmtime::{
    Engine, Store,
    component::{Component, Linker, Val},
};

pub const SOURCE: &str = r#"
(def saved (atom nil))
(def ^:export amend (fn [xs]
  (reset! saved xs)
  (mapv (fn [m] (assoc m "seen" (inc (get m "n")))) xs)))
(def ^:export recall (fn [] @saved))
"#;
pub const WIT: &str = "package test:public-map-values; world api {
  export amend: func(xs: list<map<string, u32>>) -> list<map<string, u32>>;
  export recall: func() -> list<map<string, u32>>;
}";

fn check(actual: &Val, amended: bool) {
    let Val::List(maps) = actual else {
        panic!("expected list: {actual:?}")
    };
    assert_eq!(maps.len(), 2);
    for (map, n) in maps.iter().zip([7_u32, u32::MAX - 1]) {
        let Val::Map(entries) = map else {
            panic!("expected map: {map:?}")
        };
        assert_eq!(entries.len(), if amended { 2 } else { 1 });
        assert!(
            entries.contains(&(Val::String("n".into()), Val::U32(n))),
            "{entries:?}"
        );
        if amended {
            assert!(
                entries.contains(&(Val::String("seen".into()), Val::U32(n + 1))),
                "{entries:?}"
            );
        }
    }
}

pub fn exercise(bytes: Vec<u8>, engine: &Engine) {
    let component = Component::new(engine, bytes).unwrap();
    assert_eq!(component.component_type().imports(engine).count(), 0);
    let mut store = Store::new(engine, ());
    store.set_fuel(100_000_000).unwrap();
    let instance = Linker::new(engine)
        .instantiate(&mut store, &component)
        .unwrap();
    let amend = instance.get_func(&mut store, "amend").unwrap();
    let recall = instance.get_func(&mut store, "recall").unwrap();
    let input = Val::List(vec![
        Val::Map(vec![
            (Val::String("n".into()), Val::U32(0)),
            (Val::String("n".into()), Val::U32(7)),
        ]),
        Val::Map(vec![(Val::String("n".into()), Val::U32(u32::MAX - 1))]),
    ]);
    for _ in 0..5 {
        let mut output = [Val::Bool(false)];
        amend
            .call(&mut store, &[input.clone()], &mut output)
            .unwrap();
        check(&output[0], true);
        store.gc(None).unwrap();
        recall.call(&mut store, &[], &mut output).unwrap();
        check(&output[0], false);
    }
}
