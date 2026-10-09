//! Full pinned lazy map/filter corpus; actual native execution remains required.
use serde_json::{json, Value};
use suss_cli::{portable_macro_data::FormBridge, portable_session::Session};
use suss_reader::forms::{Form, Kind};

const FIXTURE: &str = include_str!("../../../tests/oracle/fixtures/lazy-transformations-probe.sus");
const CASES: &str = include_str!("../../../tests/oracle/lazy-transformation-cases.json");
const IDS: [&str; 41] = [
    "map-demand-0",
    "map-demand-1",
    "map-demand-31",
    "map-demand-32",
    "map-demand-33",
    "map-demand-64",
    "map-demand-65",
    "map-nonchunk-demand",
    "map-nil-false",
    "map-two-input-shortest",
    "map-three-input-shortest",
    "map-four-input-variadic",
    "map-five-input-variadic",
    "map-transducer-all-reducer-arities",
    "filter-transducer-all-reducer-arities",
    "filter-nonchunk-nil-false-zero",
    "filter-chunk-empty-prefix",
    "filter-chunk-all-rejected",
    "filter-zero-is-truthy",
    "map-throw-false-chunk-throw-retry",
    "filter-throw-nil-chunk-throw-retry",
    "map-reduced-first",
    "filter-reduced-first",
    "chunk-buffer-handoff",
    "map-ordered-2-inputs",
    "map-ordered-3-inputs",
    "map-ordered-4-inputs",
    "chunk-buffer-sharing-finalization",
    "map-transducer-reduced-identity",
    "filter-transducer-reduced-identity",
    "map-transducer-callback-throw-order",
    "filter-transducer-reducer-throw-order",
    "map-live-first-helper",
    "filter-live-first-helper",
    "map-chunk-read-mutation",
    "map-live-recursive-helper",
    "map-live-chunk-helper",
    "filter-chunk-read-mutation",
    "filter-live-recursive-helper",
    "filter-live-chunk-helper",
    "map-transducer-live-variadic-apply",
];

// Decode actual canonical storage through the existing bounded FormBridge,
// without guest equality or printing. Unknown result kinds fail, never wildcard.
fn tagged(form: &Form) -> Value {
    assert!(form.metadata.is_empty(), "unexpected observation metadata");
    match &form.kind {
        Kind::Number(n) => json!({"tag":"f64", "bits":format!("{:016x}", n.to_bits())}),
        Kind::Nil => json!({"tag":"nil"}),
        Kind::Bool(v) => json!({"tag":"bool", "value":v}),
        Kind::Vector(items) => {
            json!({"tag":"vector", "items":items.iter().map(tagged).collect::<Vec<_>>()})
        }
        other => panic!("unsupported actual observation kind: {other:?}"),
    }
}
fn observe(session: &mut Session, bridge: &FormBridge, source: &str) -> Value {
    let value = session
        .eval(source)
        .unwrap_or_else(|e| panic!("{source}: {e}"));
    session.collect().unwrap();
    tagged(&bridge.read(session, &value, 0..source.len()).unwrap())
}
#[test]
fn lazy_transformations_match_complete_pinned_observations_in_both_phases_after_gc() {
    let corpus: Value = serde_json::from_str(CASES).unwrap();
    assert_eq!(corpus["schema"], 1);
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), IDS.len());
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        // Finite existing sequence stress policy; max input is 65 elements.
        // This allowance is authored, not measured/proven sufficient yet.
        session.set_operation_fuel(100_000_000);
        session.eval(FIXTURE).unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        for (case, id) in cases.iter().zip(IDS) {
            assert_eq!(case["id"], id);
            let actual = observe(&mut session, &bridge, case["source"].as_str().unwrap());
            assert_eq!(
                actual,
                case["expected"],
                "phase={:?}, case={id}",
                session.phase()
            );
        }
    }
}

#[test]
fn unforced_transformations_keep_callbacks_and_captures_across_gc_and_fragments() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        session.eval(FIXTURE).unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        session.eval("(def retained-map (let [capture [10]] (map (fn [x] (set! lazy-map-count (inc lazy-map-count)) (+ x (nth capture 0))) (list 1 2)))) (def retained-filter (let [capture [2]] (filter (fn [x] (set! lazy-filter-count (inc lazy-filter-count)) (>= x (nth capture 0))) (list 1 2 3))))").unwrap();
        session.collect().unwrap();
        assert_eq!(
            observe(&mut session, &bridge, "[lazy-map-count lazy-filter-count]"),
            json!({"tag":"vector", "items":[
                {"tag":"f64", "bits":"0000000000000000"},
                {"tag":"f64", "bits":"0000000000000000"}]}),
        );
        assert_eq!(
            observe(
                &mut session,
                &bridge,
                "[(first retained-map) (first retained-filter) lazy-map-count lazy-filter-count]"
            ),
            json!({"tag":"vector", "items":[
                {"tag":"f64", "bits":"4026000000000000"},
                {"tag":"f64", "bits":"4000000000000000"},
                {"tag":"f64", "bits":"3ff0000000000000"},
                {"tag":"f64", "bits":"4000000000000000"}]}),
        );
        session.collect().unwrap();
        assert_eq!(
            observe(
                &mut session,
                &bridge,
                "[(nth retained-map 1) (nth retained-filter 1) lazy-map-count lazy-filter-count]"
            ),
            json!({"tag":"vector", "items":[
                {"tag":"f64", "bits":"4028000000000000"},
                {"tag":"f64", "bits":"4008000000000000"},
                {"tag":"f64", "bits":"4000000000000000"},
                {"tag":"f64", "bits":"4008000000000000"}]}),
        );
        // Demanding cached heads after realizing tails must not rerun either callback.
        assert_eq!(
            observe(
                &mut session,
                &bridge,
                "[(first retained-map) (first retained-filter) lazy-map-count lazy-filter-count]"
            ),
            json!({"tag":"vector", "items":[
                {"tag":"f64", "bits":"4026000000000000"},
                {"tag":"f64", "bits":"4000000000000000"},
                {"tag":"f64", "bits":"4000000000000000"},
                {"tag":"f64", "bits":"4008000000000000"}]}),
        );
    }
}

#[test]
fn unforced_transformations_survive_with_only_host_handles() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        session.eval(FIXTURE).unwrap();
        let mapped = session.eval("(let [capture [10]] (map (fn [x] (set! lazy-map-count (inc lazy-map-count)) (+ x (nth capture 0))) (list 1 2)))").unwrap();
        let filtered = session.eval("(let [capture [2]] (filter (fn [x] (set! lazy-filter-count (inc lazy-filter-count)) (>= x (nth capture 0))) (list 1 2 3)))").unwrap();
        session.collect().unwrap();
        let project = session.eval("(fn [coll] (into [] coll))").unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        for (handle, bits) in [
            (&mapped, ["4026000000000000", "4028000000000000"]),
            (&filtered, ["4000000000000000", "4008000000000000"]),
        ] {
            let result = session.invoke(&project, &[handle]).unwrap();
            session.collect().unwrap();
            assert_eq!(
                tagged(&bridge.read(&mut session, &result, 0..0).unwrap()),
                json!({"tag":"vector", "items":[
                    {"tag":"f64", "bits":bits[0]},
                    {"tag":"f64", "bits":bits[1]}]})
            );
        }
        assert_eq!(
            observe(&mut session, &bridge, "[lazy-map-count lazy-filter-count]"),
            json!({"tag":"vector", "items":[
                {"tag":"f64", "bits":"4000000000000000"},
                {"tag":"f64", "bits":"4008000000000000"}]})
        );
    }
}

#[test]
fn handed_chunk_preserves_actual_backing_array_identity_after_gc() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        let storage = session.eval("(let [buffer (cljs.core/chunk-buffer 2) backing (.-buf buffer)] (cljs.core/chunk-append buffer nil) (cljs.core/chunk-append buffer false) (array backing (cljs.core/chunk buffer)))").unwrap();
        session.collect().unwrap();
        session
            .inspect(&storage, |mut store, value| {
                let outer = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                let fields = outer
                    .field(&mut store, 1)?
                    .unwrap_anyref()
                    .unwrap()
                    .as_array(&store)?
                    .unwrap();
                assert_eq!(
                    fields.len(&store)?,
                    2,
                    "array owner and ordinary properties"
                );
                assert_eq!(
                    fields
                        .get(&mut store, 1)?
                        .unwrap_anyref()
                        .unwrap()
                        .as_i31(&store)?
                        .unwrap()
                        .get_u32(),
                    0
                );
                let backing_storage = fields
                    .get(&mut store, 0)?
                    .unwrap_anyref()
                    .unwrap()
                    .as_array(&store)?
                    .unwrap();
                assert_eq!(
                    backing_storage.len(&store)?,
                    4,
                    "length, sparse chain, dense values, presence"
                );
                let length = backing_storage
                    .get(&mut store, 0)?
                    .unwrap_anyref()
                    .unwrap()
                    .as_struct(&store)?
                    .unwrap()
                    .field(&mut store, 0)?;
                assert_eq!(length.unwrap_f64().to_bits(), 2.0_f64.to_bits());
                assert_eq!(
                    backing_storage
                        .get(&mut store, 1)?
                        .unwrap_anyref()
                        .unwrap()
                        .as_i31(&store)?
                        .unwrap()
                        .get_u32(),
                    0
                );
                let presence = backing_storage
                    .get(&mut store, 3)?
                    .unwrap_anyref()
                    .unwrap()
                    .as_array(&store)?
                    .unwrap();
                assert_eq!(presence.len(&store)?, 2);
                for index in 0..2 {
                    assert_eq!(presence.get(&mut store, index)?.unwrap_i32(), 1);
                }
                let items = backing_storage
                    .get(&mut store, 2)?
                    .unwrap_anyref()
                    .unwrap()
                    .as_array(&store)?
                    .unwrap();
                assert_eq!(items.len(&store)?, 2);
                let backing = items
                    .get(&mut store, 0)?
                    .unwrap_anyref()
                    .unwrap()
                    .as_eqref(&mut store)?
                    .unwrap();
                let chunk = items
                    .get(&mut store, 1)?
                    .unwrap_anyref()
                    .unwrap()
                    .as_struct(&store)?
                    .unwrap();
                assert_eq!(
                    chunk.fields(&mut store)?.count(),
                    4,
                    "canonical nominal object"
                );
                let chunk_fields = chunk
                    .field(&mut store, 1)?
                    .unwrap_anyref()
                    .unwrap()
                    .as_array(&store)?
                    .unwrap();
                assert_eq!(
                    chunk_fields.len(&store)?,
                    3,
                    "complete ArrayChunk arr/off/end fields"
                );
                let handed_backing = chunk_fields
                    .get(&mut store, 0)?
                    .unwrap_anyref()
                    .unwrap()
                    .as_eqref(&mut store)?
                    .unwrap();
                assert!(wasmtime::Rooted::ref_eq(&store, &backing, &handed_backing)?);
                Ok(())
            })
            .unwrap();
    }
}
