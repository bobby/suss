//! Full source authoring evidence. Native compilation/execution/bootstrap pending.
#[path = "../../suss-compile/tests/support/portable_decode.rs"]
mod portable_decode;
use portable_decode::{Decoder, Observation};
use serde_json::{Value, json};
use suss_cli::portable_session::{Session, SessionError, SessionOptions, SessionValue};
use suss_compile::portable::resolve::Phase;
use wasmtime::{AnyRef, OwnedRooted, Val};

fn setup(phase: Phase) -> Session {
    let mut session = Session::with_options_in(SessionOptions::default(), phase).unwrap();
    session.set_operation_fuel(100_000_000);
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("user").unwrap();
    session.eval("(def sorted-effects 0)").unwrap();
    session
}
fn value_json(value: Observation) -> Value {
    match value {
        Observation::Nil => json!({"tag":"nil"}),
        Observation::Bool(value) => json!({"tag":"bool","value":value}),
        Observation::Number(bits) => json!({"tag":"number","bits":format!("{bits:016x}")}),
        Observation::String(units) => json!({"tag":"string","units":units}),
        Observation::Vector(items) => {
            json!({"tag":"vector","items":items.into_iter().map(value_json).collect::<Vec<_>>()})
        }
        other => panic!("Unexpected closed observation: {other:?}"),
    }
}
fn error_json(
    session: &mut Session,
    value: &SessionValue,
    descriptor: &OwnedRooted<AnyRef>,
    owner_type: &wasmtime::StructType,
) -> Value {
    session
        .inspect(value, |mut store, value| {
            let owner = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            assert_eq!(owner.fields(&mut store)?.count(), 5);
            assert!(wasmtime::StructType::eq(&owner.ty(&store)?, owner_type));
            let actual_descriptor = owner.field(&mut store, 0)?;
            let actual_descriptor = actual_descriptor.unwrap_anyref().unwrap();
            assert!(wasmtime::Rooted::ref_eq(
                &store,
                descriptor,
                actual_descriptor
            )?);
            let schema = actual_descriptor.as_struct(&store)?.unwrap();
            assert_eq!(schema.fields(&mut store)?.count(), 5);
            assert_eq!(schema.field(&mut store, 0)?.unwrap_i64(), 7);
            let message = owner.field(&mut store, 1)?;
            let message = message.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            assert!(matches!(
                message.ty(&store)?.element_type(),
                wasmtime::StorageType::I16
            ));
            assert!(message.len(&store)? <= 4096);
            let units = message
                .elems(&mut store)?
                .map(|v| v.unwrap_i32() as u16)
                .collect::<Vec<_>>();
            for index in [2, 3] {
                let field = owner.field(&mut store, index)?;
                assert_eq!(
                    field
                        .unwrap_anyref()
                        .unwrap()
                        .as_i31(&store)?
                        .unwrap()
                        .get_u32(),
                    0
                );
            }
            Ok(json!({"tag":"error","class":"Error","message":units}))
        })
        .unwrap()
}
#[test]
fn complete_sorted_values_iterator_returns_and_range_arities_match_pinned_raw_values() {
    let corpus: Value =
        serde_json::from_str(include_str!("../../../tests/oracle/sorted-tree-cases.json")).unwrap();
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 142);
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut session = setup(phase);
        let error_anchor = session.eval("(suss.bootstrap/error \"anchor\")").unwrap();
        let (descriptor, owner_type) = session
            .inspect(&error_anchor, |mut store, value| {
                let owner = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                let descriptor = owner
                    .field(&mut store, 0)?
                    .unwrap_anyref()
                    .unwrap()
                    .to_owned_rooted(&mut store)?;
                Ok((descriptor, owner.ty(&store)?))
            })
            .unwrap();
        for case in cases {
            session.eval("(set! sorted-effects 0)").unwrap();
            let actual = match (
                case["mode"].as_str().unwrap(),
                session.eval(
                    &case["source"]
                        .as_str()
                        .unwrap()
                        .replace("(js/Error.", "(suss.bootstrap/error"),
                ),
            ) {
                ("return", Ok(v)) | ("throw", Err(SessionError::Language(v))) => v,
                (_, result) => panic!("{} unexpected return/throw: {result:?}", case["id"]),
            };
            session.collect().unwrap();
            let observed = if case["expected"]["value"]["tag"] == "error" {
                error_json(&mut session, &actual, &descriptor, &owner_type)
            } else {
                // A fresh bounded decoder per observation: no lifetime budget
                // replenishment within a decode and no guest printer/equality.
                let mut decoder = Decoder::capture(&mut session, 20_000).unwrap();
                value_json(decoder.decode_session(&mut session, &actual).unwrap())
            };
            assert_eq!(observed, case["expected"]["value"], "{}", case["id"]);
            let trace = session.eval("sorted-effects").unwrap();
            session.collect().unwrap();
            let mut decoder = Decoder::capture(&mut session, 128).unwrap();
            assert_eq!(
                value_json(decoder.decode_session(&mut session, &trace).unwrap()),
                case["expected"]["trace"],
                "{} trace",
                case["id"]
            );
            let recovery = session.eval("(+ 17 43)").unwrap();
            let mut decoder = Decoder::capture(&mut session, 128).unwrap();
            assert_eq!(
                value_json(decoder.decode_session(&mut session, &recovery).unwrap()),
                json!({"tag":"number","bits":"404e000000000000"})
            );
        }
    }
}

// Resolve each nested source field from the actual descriptor schema.
fn field(
    session: &mut Session,
    anchor: &SessionValue,
    path: &[(&[&str], &str)],
) -> OwnedRooted<AnyRef> {
    session
        .inspect(anchor, |mut store, mut value| {
            for (names, wanted) in path {
                let owner = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                assert_eq!(owner.fields(&mut store)?.count(), 4);
                let descriptor = owner
                    .field(&mut store, 0)?
                    .unwrap_anyref()
                    .unwrap()
                    .as_struct(&store)?
                    .unwrap();
                assert_eq!(descriptor.fields(&mut store)?.count(), 5);
                let schema = descriptor
                    .field(&mut store, 1)?
                    .unwrap_anyref()
                    .unwrap()
                    .as_array(&store)?
                    .unwrap();
                assert_eq!(schema.len(&store)?, names.len() as u32);
                let slots = owner
                    .field(&mut store, 1)?
                    .unwrap_anyref()
                    .unwrap()
                    .as_array(&store)?
                    .unwrap();
                assert_eq!(slots.len(&store)?, names.len() as u32);
                let mut observed = Vec::new();
                for index in 0..schema.len(&store)? {
                    let name = schema
                        .get(&mut store, index)?
                        .unwrap_anyref()
                        .unwrap()
                        .as_array(&store)?
                        .unwrap();
                    assert!(name.len(&store)? <= 32);
                    assert!(matches!(
                        name.ty(&store)?.element_type(),
                        wasmtime::StorageType::I16
                    ));
                    let units = name
                        .elems(&mut store)?
                        .map(|v| v.unwrap_i32() as u16)
                        .collect::<Vec<_>>();
                    observed.push(String::from_utf16(&units).unwrap());
                }
                assert_eq!(
                    observed.iter().map(String::as_str).collect::<Vec<_>>(),
                    *names
                );
                value = slots.get(
                    &mut store,
                    names.iter().position(|n| *n == *wanted).unwrap() as u32,
                )?;
            }
            value.unwrap_anyref().unwrap().to_owned_rooted(&mut store)
        })
        .unwrap()
}
fn same(
    session: &mut Session,
    anchor: &SessionValue,
    a: &OwnedRooted<AnyRef>,
    b: &OwnedRooted<AnyRef>,
) -> bool {
    session
        .inspect(anchor, |store, _| wasmtime::Rooted::ref_eq(&store, a, b))
        .unwrap()
}
const MAP_FIELDS: &[&str] = &["comp", "tree", "cnt", "meta", "__hash"];
const NODE_FIELDS: &[&str] = &["key", "val", "left", "right", "__hash"];
#[test]
fn tree_map_update_sharing_comparator_identity_and_old_tree_survive_gc() {
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut s = setup(phase);
        let original = s
            .eval("(def sorted-old (sorted-map 2 20 1 10 3 30)) sorted-old")
            .unwrap();
        let comp_root = field(&mut s, &original, &[(MAP_FIELDS, "comp")]);
        let tree_root = field(&mut s, &original, &[(MAP_FIELDS, "tree")]);
        let left_root = field(
            &mut s,
            &original,
            &[(MAP_FIELDS, "tree"), (NODE_FIELDS, "left")],
        );
        let updated = s.eval("(assoc sorted-old 4 40)").unwrap();
        s.collect().unwrap();
        let next_tree = field(&mut s, &updated, &[(MAP_FIELDS, "tree")]);
        assert!(!same(&mut s, &updated, &next_tree, &tree_root));
        let next_left = field(
            &mut s,
            &updated,
            &[(MAP_FIELDS, "tree"), (NODE_FIELDS, "left")],
        );
        assert!(!same(&mut s, &updated, &next_left, &left_root));
        let nested_left = field(
            &mut s,
            &updated,
            &[
                (MAP_FIELDS, "tree"),
                (NODE_FIELDS, "left"),
                (NODE_FIELDS, "left"),
            ],
        );
        assert!(same(&mut s, &updated, &nested_left, &left_root));
        let reference: Value=serde_json::from_str(include_str!("../../../docs/compatibility/sorted-tree-collections/evidence/sharing-repair/observations.json")).unwrap();
        let mut classes = Vec::new();
        for (name, source) in [
            ("BlackNode", "(BlackNode. 0 0 nil nil nil)"),
            ("RedNode", "(RedNode. 0 0 nil nil nil)"),
        ] {
            let anchor = s.eval(source).unwrap();
            let (descriptor, ty) = s
                .inspect(&anchor, |mut store, value| {
                    let owner = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                    Ok((
                        owner
                            .field(&mut store, 0)?
                            .unwrap_anyref()
                            .unwrap()
                            .to_owned_rooted(&mut store)?,
                        owner.ty(&store)?,
                    ))
                })
                .unwrap();
            classes.push((anchor, descriptor, ty, name));
        }
        s.collect().unwrap();
        assert_rotation_shape(
            &mut s,
            &original,
            &[(MAP_FIELDS, "tree")],
            &reference["old"],
            &classes,
            &mut Vec::new(),
        );
        assert_rotation_shape(
            &mut s,
            &updated,
            &[(MAP_FIELDS, "tree")],
            &reference["updated"],
            &classes,
            &mut Vec::new(),
        );
        let next_comp = field(&mut s, &updated, &[(MAP_FIELDS, "comp")]);
        assert!(same(&mut s, &updated, &next_comp, &comp_root));
        let old_root = s
            .inspect(&original, |mut store, value| {
                value.unwrap_anyref().unwrap().to_owned_rooted(&mut store)
            })
            .unwrap();
        for source in ["(assoc sorted-old 2 20)", "(dissoc sorted-old 99)"] {
            let unchanged = s.eval(source).unwrap();
            s.collect().unwrap();
            let new_root = s
                .inspect(&unchanged, |mut store, value| {
                    value.unwrap_anyref().unwrap().to_owned_rooted(&mut store)
                })
                .unwrap();
            assert!(same(&mut s, &unchanged, &new_root, &old_root));
        }
        let cnt = field(&mut s, &original, &[(MAP_FIELDS, "cnt")]);
        s.inspect(&original, |mut store, _| {
            let rooted = cnt.to_rooted(&mut store);
            let number = rooted.as_struct(&store)?.unwrap();
            assert_eq!(number.fields(&mut store)?.count(), 1);
            let Val::F64(bits) = number.field(&mut store, 0)? else {
                panic!("raw count Number")
            };
            assert_eq!(bits, 3.0_f64.to_bits());
            Ok(())
        })
        .unwrap();
        let cmp_throw =
            s.eval("(sorted-map-by (fn [a b] (set! sorted-effects 1) (throw 73)) 1 10 2 20)");
        let Err(SessionError::Language(payload)) = cmp_throw else {
            panic!("original comparator throw")
        };
        s.collect().unwrap();
        let mut decoder = Decoder::capture(&mut s, 128).unwrap();
        assert_eq!(
            value_json(decoder.decode_session(&mut s, &payload).unwrap()),
            json!({"tag":"number","bits":"4052400000000000"})
        );
        let trace = s.eval("sorted-effects").unwrap();
        let mut decoder = Decoder::capture(&mut s, 128).unwrap();
        assert_eq!(
            value_json(decoder.decode_session(&mut s, &trace).unwrap()),
            json!({"tag":"number","bits":"3ff0000000000000"})
        );
    }
}

#[test]
fn additional_typed_protocol_pairs_ignore_public_rebinding_and_retain_capture_order() {
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut session = setup(phase);
        session
            .eval(
                "(deftype SortedDispatchProbe [x]
            IEquiv (-equiv [this other] x)
            ILookup (-lookup [this k] x) (-lookup [this k not-found] not-found)
            IAssociative (-assoc [this k v] x) (-contains-key? [this k] true)
            ISeq (-first [this] x) (-rest [this] nil)
            INext (-next [this] nil)
            IMapEntry (-key [this] x) (-val [this] x))",
            )
            .unwrap();
        for (protocol, method, args, expected) in [
            (
                "IEquiv",
                "-equiv",
                "nil",
                json!({"tag":"number","bits":"401c000000000000"}),
            ),
            (
                "ILookup",
                "-lookup",
                "0",
                json!({"tag":"number","bits":"401c000000000000"}),
            ),
            (
                "ILookup",
                "-lookup",
                "0 17",
                json!({"tag":"number","bits":"4031000000000000"}),
            ),
            (
                "IAssociative",
                "-assoc",
                "0 17",
                json!({"tag":"number","bits":"401c000000000000"}),
            ),
            (
                "ISeq",
                "-first",
                "",
                json!({"tag":"number","bits":"401c000000000000"}),
            ),
            ("INext", "-next", "", json!({"tag":"nil"})),
            (
                "IMapEntry",
                "-val",
                "",
                json!({"tag":"number","bits":"401c000000000000"}),
            ),
        ] {
            let source = format!(
                "(let [saved {method}] (set! {method} (fn [& xs] (throw 73))) (try (suss.bootstrap/direct-protocol-call {protocol} {method} (SortedDispatchProbe. 7) {args}) (finally (set! {method} saved))))"
            );
            let result = session.eval(&source).unwrap();
            session.collect().unwrap();
            let mut decoder = Decoder::capture(&mut session, 128).unwrap();
            assert_eq!(
                value_json(decoder.decode_session(&mut session, &result).unwrap()),
                expected
            );
        }
        let value = session
            .eval(
                "(suss.bootstrap/direct-protocol-call ILookup -lookup
            (do (set! sorted-effects 1) (SortedDispatchProbe. 7))
            (do (set! sorted-effects (+ (* sorted-effects 10) 2))
                (extend-type SortedDispatchProbe ILookup
                    (-lookup [this k] 9) (-lookup [this k not-found] not-found)) 0))",
            )
            .unwrap();
        session.collect().unwrap();
        let mut decoder = Decoder::capture(&mut session, 128).unwrap();
        assert_eq!(
            value_json(decoder.decode_session(&mut session, &value).unwrap()),
            json!({"tag":"number","bits":"401c000000000000"})
        );
        let trace = session.eval("sorted-effects").unwrap();
        let mut decoder = Decoder::capture(&mut session, 128).unwrap();
        assert_eq!(
            value_json(decoder.decode_session(&mut session, &trace).unwrap()),
            json!({"tag":"number","bits":"4028000000000000"})
        );
        let value = session
            .eval(
                "(suss.bootstrap/direct-protocol-call ILookup -lookup (SortedDispatchProbe. 7) 0)",
            )
            .unwrap();
        let mut decoder = Decoder::capture(&mut session, 128).unwrap();
        assert_eq!(
            value_json(decoder.decode_session(&mut session, &value).unwrap()),
            json!({"tag":"number","bits":"4022000000000000"})
        );
        for source in [
            "(suss.bootstrap/direct-protocol-call ILookup -lookup (SortedDispatchProbe. 7))",
            "(suss.bootstrap/direct-protocol-call ILookup -lookup (SortedDispatchProbe. 7) 0 nil nil)",
            "(suss.bootstrap/direct-protocol-call ISeq -first (SortedDispatchProbe. 7) 0)",
            "(suss.bootstrap/direct-protocol-call ISeq -rest (SortedDispatchProbe. 7))",
            "(suss.bootstrap/direct-protocol-call cljs.core/ILookup -lookup (SortedDispatchProbe. 7) 0)",
        ] {
            assert!(session.eval(source).is_err(), "{source}");
        }
    }
}

// Independently compare every actual canonical node, source field and exact
// numeric bits with the freshly observed old/new rotation trees.
fn assert_rotation_shape(
    session: &mut Session,
    anchor: &SessionValue,
    path: &[(&[&str], &str)],
    expected: &Value,
    classes: &[(
        SessionValue,
        OwnedRooted<AnyRef>,
        wasmtime::StructType,
        &str,
    )],
    ancestors: &mut Vec<OwnedRooted<AnyRef>>,
) {
    assert!(ancestors.len() < 64);
    let actual = field(session, anchor, path);
    session
        .inspect(anchor, |mut store, _| {
            let rooted = actual.to_rooted(&mut store);
            if expected.is_null() {
                assert_eq!(rooted.as_i31(&store)?.expect("nil tree child").get_u32(), 0);
                return Ok(());
            }
            for parent in ancestors.iter() {
                assert!(
                    !wasmtime::Rooted::ref_eq(&store, parent, &actual)?,
                    "tree cycle"
                );
            }
            let owner = rooted.as_struct(&store)?.expect("node storage");
            assert_eq!(owner.fields(&mut store)?.count(), 4);
            let descriptor = owner.field(&mut store, 0)?;
            let descriptor = descriptor.unwrap_anyref().unwrap();
            let class = classes
                .iter()
                .find_map(|(_, known, ty, name)| {
                    if wasmtime::Rooted::ref_eq(&store, known, descriptor).unwrap() {
                        Some((ty, *name))
                    } else {
                        None
                    }
                })
                .expect("canonical node class");
            assert!(wasmtime::StructType::eq(&owner.ty(&store)?, class.0));
            assert_eq!(class.1, expected["class"].as_str().unwrap());
            Ok(())
        })
        .unwrap();
    if expected.is_null() {
        return;
    }
    ancestors.push(actual);
    for name in ["key", "val"] {
        let mut child_path = path.to_vec();
        child_path.push((NODE_FIELDS, name));
        let value = field(session, anchor, &child_path);
        session
            .inspect(anchor, |mut store, _| {
                let rooted = value.to_rooted(&mut store);
                let number = rooted.as_struct(&store)?.expect("raw Number");
                assert_eq!(number.fields(&mut store)?.count(), 1);
                let Val::F64(bits) = number.field(&mut store, 0)? else {
                    panic!("raw Number layout")
                };
                assert_eq!(bits, (expected[name].as_i64().unwrap() as f64).to_bits());
                Ok(())
            })
            .unwrap();
    }
    let mut hash_path = path.to_vec();
    hash_path.push((NODE_FIELDS, "__hash"));
    let hash = field(session, anchor, &hash_path);
    session
        .inspect(anchor, |mut store, _| {
            assert!(expected["hash"].is_null());
            let rooted = hash.to_rooted(&mut store);
            assert_eq!(rooted.as_i31(&store)?.expect("nil hash").get_u32(), 0);
            Ok(())
        })
        .unwrap();
    for name in ["left", "right"] {
        let mut child_path = path.to_vec();
        child_path.push((NODE_FIELDS, name));
        assert_rotation_shape(
            session,
            anchor,
            &child_path,
            &expected[name],
            classes,
            ancestors,
        );
    }
    ancestors.pop();
}
