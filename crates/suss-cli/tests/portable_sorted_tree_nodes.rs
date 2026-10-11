//! Complete tree nodes; authored tests, compilation/bootstrap/native execution pending.
use suss_cli::portable_session::{Session, SessionError, SessionOptions, SessionValue};
use suss_compile::portable::resolve::Phase;
use wasmtime::{AsContextMut, Val};
const IDS: &[&str] = &[
    "balance-left-nonred",
    "balance-left-red-no-red-child",
    "balance-left-outer-red",
    "balance-left-inner-red",
    "balance-left-outer-priority",
    "balance-right-nonred",
    "balance-right-red-no-red-child",
    "balance-right-outer-red",
    "balance-right-inner-red",
    "balance-right-outer-priority",
    "left-del-red",
    "right-del-red",
    "left-del-black-sibling",
    "right-del-black-sibling",
    "left-del-red-black-inner",
    "right-del-red-black-inner",
    "balance-left-del-invariant",
    "balance-right-del-invariant",
    "blacknode-key-val-count",
    "blacknode-nth-get-invoke",
    "blacknode-nth-error",
    "blacknode-invoke-error",
    "blacknode-meta-with-meta",
    "blacknode-stack-conj-empty",
    "blacknode-seq-rseq",
    "blacknode-assoc-vector",
    "blacknode-contains-find",
    "blacknode-equiv",
    "blacknode-reduce",
    "blacknode-indexOf",
    "blacknode-replace-color",
    "blacknode-hash-cache-zero",
    "blacknode-hash-cache-success",
    "blacknode-hash-cache-throw",
    "rednode-key-val-count",
    "rednode-nth-get-invoke",
    "rednode-nth-error",
    "rednode-invoke-error",
    "rednode-meta-with-meta",
    "rednode-stack-conj-empty",
    "rednode-seq-rseq",
    "rednode-assoc-vector",
    "rednode-contains-find",
    "rednode-equiv",
    "rednode-reduce",
    "rednode-indexOf",
    "rednode-replace-color",
    "rednode-hash-cache-zero",
    "rednode-hash-cache-success",
    "rednode-hash-cache-throw",
    "black-blacken-owner-identity",
    "black-redden-sharing",
    "red-blacken-sharing",
    "red-redden-invariant",
    "red-add-remove-children",
    "black-method-insertion-dispatch",
    "red-method-balance-left-outer",
    "red-method-balance-left-inner",
    "red-method-balance-left-none",
    "red-method-balance-right-outer",
    "red-method-balance-right-inner",
    "red-method-balance-right-none",
    "kv-inorder-effects",
    "kv-left-reduced-preserves-wrapper",
    "kv-root-reduced-preserves-wrapper",
    "kv-initial-reduced-leftless",
    "kv-original-throw",
    "blacknode-kv-method",
    "rednode-kv-method",
    "blacknode-typed-nth-ignores-public-rebind",
    "blacknode-typed-assoc-n-ignores-public-rebind",
    "rednode-typed-nth-ignores-public-rebind",
    "rednode-typed-assoc-n-ignores-public-rebind",
    "comparable-symbol",
    "comparable-keyword",
    "comparable-subvec",
    "comparable-vector",
    "comparable-mapentry",
    "blacknode-comparable-vector",
    "blacknode-comparable-other-node",
    "blacknode-comparable-live-delegate",
    "blacknode-comparable-rejection-format-order",
    "rednode-comparable-vector",
    "rednode-comparable-other-node",
    "rednode-comparable-live-delegate",
    "rednode-comparable-rejection-format-order",
];
fn setup(phase: Phase) -> Session {
    let mut session = Session::with_options_in(SessionOptions::default(), phase).unwrap();
    session.set_operation_fuel(100_000_000);
    session
        .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
        .unwrap();
    session.enter_namespace("user").unwrap();
    session
        .eval(include_str!(
            "../../../tests/oracle/fixtures/tree-node-probe.sus"
        ))
        .unwrap();
    session.collect().unwrap();
    session
}
fn number_bits(session: &mut Session, value: &SessionValue) -> u64 {
    session
        .inspect(value, |mut store, value| {
            let number = value
                .unwrap_anyref()
                .unwrap()
                .as_struct(&store)?
                .expect("Number");
            let fields = number.fields(&mut store)?.collect::<Vec<_>>();
            let [Val::F64(bits)] = fields.as_slice() else {
                panic!("boxed Number layout")
            };
            Ok(*bits)
        })
        .unwrap()
}
#[test]
fn complete_tree_nodes_match_ordered_pinned_value_and_effect_cases_after_gc() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/oracle/tree-node-cases.json")).unwrap();
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), IDS.len());
    for (case, id) in cases.iter().zip(IDS) {
        assert_eq!(case["id"], *id);
        assert_eq!(
            case["expected"],
            serde_json::json!({"tag":"bool","value":true})
        );
    }
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut session = setup(phase);
        for case in cases {
            let source = case["source"].as_str().unwrap();
            let value = session
                .eval(source)
                .unwrap_or_else(|e| panic!("{}: {source}: {e}", case["id"]));
            session.collect().unwrap();
            session
                .inspect(&value, |store, value| {
                    let raw = value
                        .unwrap_anyref()
                        .unwrap()
                        .as_i31(&store)?
                        .expect("Boolean ABI")
                        .get_u32();
                    assert_eq!(raw, 4, "{}", case["id"]);
                    Ok(())
                })
                .unwrap();
        }
    }
}

// Resolve source slots through the actual descriptor schema, never guessed
// source field offsets. Call only inside inspect so every reference is rooted.
macro_rules! source_field {
    ($store:ident, $value:expr, $names:expr, $wanted:expr) => {{
        let owner = $value.unwrap_anyref().unwrap().as_struct(&$store)?.unwrap();
        assert_eq!(owner.fields(&mut $store)?.count(), 4);
        let descriptor = owner
            .field(&mut $store, 0)?
            .unwrap_anyref()
            .unwrap()
            .as_struct(&$store)?
            .unwrap();
        assert_eq!(descriptor.fields(&mut $store)?.count(), 5);
        let schema = descriptor
            .field(&mut $store, 1)?
            .unwrap_anyref()
            .unwrap()
            .as_array(&$store)?
            .unwrap();
        let slots = owner
            .field(&mut $store, 1)?
            .unwrap_anyref()
            .unwrap()
            .as_array(&$store)?
            .unwrap();
        assert_eq!(schema.len(&$store)?, $names.len() as u32);
        assert_eq!(slots.len(&$store)?, schema.len(&$store)?);
        let mut names = Vec::new();
        for i in 0..schema.len(&$store)? {
            let text = schema
                .get(&mut $store, i)?
                .unwrap_anyref()
                .unwrap()
                .as_array(&$store)?
                .unwrap();
            let units = text
                .elems(&mut $store)?
                .map(|v| v.unwrap_i32() as u16)
                .collect::<Vec<_>>();
            names.push(String::from_utf16(&units).unwrap());
        }
        assert_eq!(
            names
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .as_slice(),
            $names
        );
        slots.get(
            &mut $store,
            names.iter().position(|n| n == $wanted).unwrap() as u32,
        )?
    }};
}
const NODE_FIELDS: &[&str] = &["key", "val", "left", "right", "__hash"];

#[test]
fn nodes_preserve_canonical_classes_child_sharing_and_cache_boundaries_after_gc() {
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut s = setup(phase);
        s.eval(
            "(def native-left (BlackNode. 1 11 nil nil 41))
                (def native-right (BlackNode. 9 99 nil nil 43))
                (def native-parent (BlackNode. 6 66 native-left native-right 47))
                (def native-red-anchor (RedNode. 0 0 nil nil nil))",
        )
        .unwrap();
        let anchor = s.eval("native-parent").unwrap();
        let black_descriptor = s
            .inspect(&anchor, |mut store, value| {
                value
                    .unwrap_anyref()
                    .unwrap()
                    .as_struct(&store)?
                    .unwrap()
                    .field(&mut store, 0)?
                    .unwrap_anyref()
                    .unwrap()
                    .to_owned_rooted(&mut store)
            })
            .unwrap();
        let anchor = s.eval("native-red-anchor").unwrap();
        let red_descriptor = s
            .inspect(&anchor, |mut store, value| {
                value
                    .unwrap_anyref()
                    .unwrap()
                    .as_struct(&store)?
                    .unwrap()
                    .field(&mut store, 0)?
                    .unwrap_anyref()
                    .unwrap()
                    .to_owned_rooted(&mut store)
            })
            .unwrap();
        s.inspect(&anchor, |store, _| {
            assert!(!wasmtime::Rooted::ref_eq(
                &store,
                &black_descriptor,
                &red_descriptor
            )?);
            Ok(())
        })
        .unwrap();
        let left = s.eval("native-left").unwrap();
        let left_root = s
            .inspect(&left, |mut store, v| {
                v.unwrap_anyref().unwrap().to_owned_rooted(&mut store)
            })
            .unwrap();
        let right = s.eval("native-right").unwrap();
        let right_root = s
            .inspect(&right, |mut store, v| {
                v.unwrap_anyref().unwrap().to_owned_rooted(&mut store)
            })
            .unwrap();
        let original = s.eval("native-parent").unwrap();
        let original_root = s
            .inspect(&original, |mut store, v| {
                v.unwrap_anyref().unwrap().to_owned_rooted(&mut store)
            })
            .unwrap();
        for (source, red, same, cache) in [
            ("(.blacken native-parent)", false, true, Some(47.0_f64)),
            ("(.redden native-parent)", true, false, None),
            (
                "(balance-left 6 66 native-left native-right)",
                false,
                false,
                None,
            ),
            (
                "(balance-right 6 66 native-left native-right)",
                false,
                false,
                None,
            ),
            (
                "(.replace native-parent 6 66 native-left native-right)",
                false,
                false,
                None,
            ),
        ] {
            let value = s.eval(source).unwrap();
            s.collect().unwrap();
            s.inspect(&value, |mut store, value| {
                let owner = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                let descriptor = owner.field(&mut store, 0)?;
                assert!(wasmtime::Rooted::ref_eq(
                    &store,
                    if red {
                        &red_descriptor
                    } else {
                        &black_descriptor
                    },
                    descriptor.unwrap_anyref().unwrap()
                )?);
                assert_eq!(
                    wasmtime::Rooted::ref_eq(
                        &store,
                        &original_root,
                        value.unwrap_anyref().unwrap()
                    )?,
                    same
                );
                for (name, root) in [("left", &left_root), ("right", &right_root)] {
                    let child = source_field!(store, value, NODE_FIELDS, name);
                    assert!(wasmtime::Rooted::ref_eq(
                        &store,
                        root,
                        child.unwrap_anyref().unwrap()
                    )?);
                }
                for (name, expected) in [("key", 6.0_f64), ("val", 66.0_f64)] {
                    let field = source_field!(store, value, NODE_FIELDS, name);
                    let number = field.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                    assert_eq!(number.fields(&mut store)?.count(), 1);
                    assert_eq!(
                        number.field(&mut store, 0)?.unwrap_f64().to_bits(),
                        expected.to_bits()
                    );
                }
                let field = source_field!(store, value, NODE_FIELDS, "__hash");
                match cache {
                    None => assert_eq!(
                        field
                            .unwrap_anyref()
                            .unwrap()
                            .as_i31(&store)?
                            .unwrap()
                            .get_u32(),
                        0
                    ),
                    Some(n) => assert_eq!(
                        field
                            .unwrap_anyref()
                            .unwrap()
                            .as_struct(&store)?
                            .unwrap()
                            .field(&mut store, 0)?
                            .unwrap_f64()
                            .to_bits(),
                        n.to_bits()
                    ),
                }
                Ok(())
            })
            .unwrap();
        }
        s.eval(
            "(def native-insertion (RedNode. 4 44
                  (RedNode. 2 22 native-left native-right 53) native-right 59))",
        )
        .unwrap();
        let rotated = s
            .eval("(balance-left 6 66 native-insertion native-right)")
            .unwrap();
        s.collect().unwrap();
        s.inspect(&rotated, |mut store, value| {
            let owner = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let owner_descriptor = owner
                .field(&mut store, 0)?
                .unwrap_anyref()
                .unwrap()
                .to_owned_rooted(&mut store)?;
            assert!(wasmtime::Rooted::ref_eq(
                &store,
                &red_descriptor,
                &owner_descriptor
            )?);
            let left_node = source_field!(store, value, NODE_FIELDS, "left");
            let right_node = source_field!(store, value, NODE_FIELDS, "right");
            for child in [&left_node, &right_node] {
                let object = child.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                let object_descriptor = object
                    .field(&mut store, 0)?
                    .unwrap_anyref()
                    .unwrap()
                    .to_owned_rooted(&mut store)?;
                assert!(wasmtime::Rooted::ref_eq(
                    &store,
                    &black_descriptor,
                    &object_descriptor
                )?);
                let cache = source_field!(store, child, NODE_FIELDS, "__hash");
                assert_eq!(
                    cache
                        .unwrap_anyref()
                        .unwrap()
                        .as_i31(&store)?
                        .unwrap()
                        .get_u32(),
                    0
                );
            }
            for (child, slot, root) in [
                (&left_node, "left", &left_root),
                (&left_node, "right", &right_root),
                (&right_node, "left", &right_root),
                (&right_node, "right", &right_root),
            ] {
                let shared = source_field!(store, child, NODE_FIELDS, slot);
                assert!(wasmtime::Rooted::ref_eq(
                    &store,
                    root,
                    shared.unwrap_anyref().unwrap()
                )?);
            }
            Ok(())
        })
        .unwrap();
        for (source, n) in [
            ("(.-__hash native-left)", 41.0_f64),
            ("(.-__hash native-right)", 43.0_f64),
            ("(.-__hash native-parent)", 47.0_f64),
            ("(.-__hash native-insertion)", 59.0_f64),
            ("(.-__hash (.-left native-insertion))", 53.0_f64),
        ] {
            let value = s.eval(source).unwrap();
            s.collect().unwrap();
            assert_eq!(number_bits(&mut s, &value), n.to_bits());
        }
    }
}

#[test]
fn tree_reduction_preserves_wrapper_identity_original_throw_and_effect_order() {
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut s = setup(phase);
        s.eval("(def tree-effects 0) (def tree-stop (reduced false))
                (def native-tree (BlackNode. 2 20 (BlackNode. 1 10 nil nil nil) (BlackNode. 3 30 nil nil nil) nil))").unwrap();
        let stop = s.eval("tree-stop").unwrap();
        let stop_root = s
            .inspect(&stop, |mut store, v| {
                v.unwrap_anyref().unwrap().to_owned_rooted(&mut store)
            })
            .unwrap();
        let value = s
            .eval(
                "(tree-map-kv-reduce native-tree
                (fn [acc k v] (set! tree-effects (+ (* tree-effects 10) k))
                   (if (= k 2) tree-stop acc)) 0)",
            )
            .unwrap();
        s.collect().unwrap();
        s.inspect(&value, |mut store, value| {
            assert!(wasmtime::Rooted::ref_eq(
                &store,
                &stop_root,
                value.unwrap_anyref().unwrap()
            )?);
            let inner = source_field!(store, value, &["val"], "val");
            assert_eq!(
                inner
                    .unwrap_anyref()
                    .unwrap()
                    .as_i31(&store)?
                    .unwrap()
                    .get_u32(),
                2
            );
            Ok(())
        })
        .unwrap();
        let effects = s.eval("tree-effects").unwrap();
        s.collect().unwrap();
        assert_eq!(number_bits(&mut s, &effects), 12.0_f64.to_bits());
        s.eval("(set! tree-effects 0)").unwrap();
        let Err(SessionError::Language(payload)) = s.eval(
            "(tree-map-kv-reduce native-tree
            (fn [acc k v] (set! tree-effects (+ (* tree-effects 10) k)) (throw 73)) 0)",
        ) else {
            panic!("original payload")
        };
        s.collect().unwrap();
        assert_eq!(number_bits(&mut s, &payload), 73.0_f64.to_bits());
        let effects = s.eval("tree-effects").unwrap();
        s.collect().unwrap();
        assert_eq!(number_bits(&mut s, &effects), 1.0_f64.to_bits());
        let recovered = s
            .eval("(tree-map-kv-reduce native-tree (fn [acc k v] (+ acc v)) 0)")
            .unwrap();
        s.collect().unwrap();
        assert_eq!(number_bits(&mut s, &recovered), 60.0_f64.to_bits());
    }
}

#[test]
fn hyphenated_object_methods_preserve_receiver_lookup_and_argument_order() {
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut s = setup(phase);
        s.eval(
            "(def method-effects 0)
            (deftype TreeMethodProbe [x] Object
              (read-value [this] x)
              (read-value [this y] (+ x y)))",
        )
        .unwrap();
        let result = s
            .eval(
                "(.read-value (do (set! method-effects 1) (TreeMethodProbe. 7))
                (do (set! method-effects (+ (* method-effects 10) 2)) 9))",
            )
            .unwrap();
        s.collect().unwrap();
        assert_eq!(number_bits(&mut s, &result), 16.0_f64.to_bits());
        let effects = s.eval("method-effects").unwrap();
        s.collect().unwrap();
        assert_eq!(number_bits(&mut s, &effects), 12.0_f64.to_bits());
        let result = s.eval("(.read-value (TreeMethodProbe. 7))").unwrap();
        s.collect().unwrap();
        assert_eq!(number_bits(&mut s, &result), 7.0_f64.to_bits());
        // ClojureScript munges the source property spelling consistently; an
        // absent field reads nil rather than becoming a compile-time error.
        let missing = s
            .eval("(nil? (.-bad-name (TreeMethodProbe. 7)))")
            .unwrap();
        assert_eq!(
            s.inspect(&missing, |store, value| {
                Ok(value.unwrap_anyref().unwrap().as_i31(&store)?.unwrap().get_u32())
            })
            .unwrap(),
            4
        );
        s.eval("(set! method-effects 0)").unwrap();
        let Err(SessionError::Language(payload)) = s.eval(
            "(.read-value
            (do (set! method-effects 1) (throw 73))
            (do (set! method-effects 2) 9))",
        ) else {
            panic!("owner throw")
        };
        s.collect().unwrap();
        assert_eq!(number_bits(&mut s, &payload), 73.0_f64.to_bits());
        let effects = s.eval("method-effects").unwrap();
        s.collect().unwrap();
        assert_eq!(number_bits(&mut s, &effects), 1.0_f64.to_bits());
    }
}

#[test]
fn this_as_restores_the_enclosing_receiver_after_nested_calls_and_language_throws() {
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut s = setup(phase);
        s.eval(
            "(deftype ThisReceiverProbe [tag] Object
               (is-self [this] (this-as receiver (identical? receiver this)))
               (nested [this other]
                 (let [inner (.is-self other)]
                   (and inner (this-as receiver (identical? receiver this)))))
               (throws [this] (throw 73))
               (catches-inner [this other]
                 (try (.throws other)
                   (catch :default error
                     (this-as receiver (identical? receiver this))))))",
        )
        .unwrap();
        for expression in [
            "(.is-self (ThisReceiverProbe. 1))",
            "(.nested (ThisReceiverProbe. 1) (ThisReceiverProbe. 2))",
            "(.catches-inner (ThisReceiverProbe. 1) (ThisReceiverProbe. 2))",
        ] {
            let result = s
                .eval(&format!("(if {expression} 1 0)"))
                .unwrap();
            s.collect().unwrap();
            assert_eq!(number_bits(&mut s, &result), 1.0_f64.to_bits(), "{expression}");
        }
    }
}

#[test]
fn direct_nominal_protocol_call_captures_live_method_before_argument_effects() {
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut s = setup(phase);
        s.eval(
            "(def direct-effects 0)
             (deftype TreeIndexedProbe [x] IIndexed
               (-nth [this n] x)
               (-nth [this n not-found] not-found))",
        )
        .unwrap();
        let value = s
            .eval(
                "(suss.bootstrap/direct-protocol-call IIndexed -nth
                (do (set! direct-effects 1) (TreeIndexedProbe. 7))
                (do (set! direct-effects (+ (* direct-effects 10) 2))
                    (extend-type TreeIndexedProbe IIndexed
                      (-nth [this n] 9)
                      (-nth [this n not-found] not-found)) 0))",
            )
            .unwrap();
        s.collect().unwrap();
        assert_eq!(number_bits(&mut s, &value), 7.0_f64.to_bits());
        let effects = s.eval("direct-effects").unwrap();
        s.collect().unwrap();
        assert_eq!(number_bits(&mut s, &effects), 12.0_f64.to_bits());
        let value = s
            .eval("(suss.bootstrap/direct-protocol-call IIndexed -nth (TreeIndexedProbe. 7) 0)")
            .unwrap();
        s.collect().unwrap();
        assert_eq!(number_bits(&mut s, &value), 9.0_f64.to_bits());
        for source in [
            "(suss.bootstrap/direct-protocol-call IIndexed unknown (TreeIndexedProbe. 7) 0)",
            "(suss.bootstrap/direct-protocol-call IIndexed -nth)",
            "(suss.bootstrap/direct-protocol-call IIndexed -nth (TreeIndexedProbe. 7))",
            "(suss.bootstrap/direct-protocol-call IIndexed -nth (TreeIndexedProbe. 7) 0 nil nil)",
            "(suss.bootstrap/direct-protocol-call NotAProtocol -nth (TreeIndexedProbe. 7) 0)",
            "(suss.bootstrap/direct-protocol-call cljs.core/IIndexed -nth (TreeIndexedProbe. 7) 0)",
        ] {
            assert!(s.eval(source).is_err(), "{source}");
        }
        s.eval("(set! direct-effects 0)").unwrap();
        let Err(SessionError::Language(payload)) = s.eval(
            "(suss.bootstrap/direct-protocol-call IIndexed -nth
            (do (set! direct-effects 1) (throw 73))
            (do (set! direct-effects 2) 0))",
        ) else {
            panic!("owner throw")
        };
        s.collect().unwrap();
        assert_eq!(number_bits(&mut s, &payload), 73.0_f64.to_bits());
        let effects = s.eval("direct-effects").unwrap();
        s.collect().unwrap();
        assert_eq!(number_bits(&mut s, &effects), 1.0_f64.to_bits());
    }
}

// Supplement the original 86 Boolean assertions with actual returned storage.
// The reference reads upstream JS fields; this decoder reads ABI storage and
// canonical descriptors. Neither path invokes guest equality or printing.
struct RawTreeClass {
    _anchor: SessionValue,
    descriptor: wasmtime::OwnedRooted<wasmtime::AnyRef>,
    owner_type: wasmtime::StructType,
    name: &'static str,
}
fn raw_tree_decode(
    store: &mut wasmtime::StoreContextMut<'_, ()>,
    value: &Val,
    classes: &[RawTreeClass],
    path: &mut Vec<wasmtime::OwnedRooted<wasmtime::AnyRef>>,
    remaining: &mut usize,
) -> wasmtime::Result<serde_json::Value> {
    use serde_json::json;
    if path.len() >= 64 || *remaining == 0 {
        return Err(wasmtime::Error::msg("raw tree depth/budget"));
    }
    *remaining -= 1;
    let reference = value
        .unwrap_anyref()
        .ok_or_else(|| wasmtime::Error::msg("raw tree null reference"))?;
    if let Some(i) = reference.as_i31(&*store)? {
        return match i.get_u32() {
            0 => Ok(json!({"tag":"nil"})),
            2 => Ok(json!({"tag":"bool","value":false})),
            4 => Ok(json!({"tag":"bool","value":true})),
            _ => Err(wasmtime::Error::msg("raw tree unsupported immediate")),
        };
    }
    let owner = reference
        .as_struct(&*store)?
        .ok_or_else(|| wasmtime::Error::msg("raw tree nonstruct"))?;
    let count = owner.fields(&mut *store)?.count();
    if count == 1 {
        return match owner.field(&mut *store, 0)? {
            Val::F64(bits) => Ok(json!({"tag":"number","bits":format!("{bits:016x}")})),
            _ => Err(wasmtime::Error::msg("raw tree number storage")),
        };
    }
    if count != 4 && count != 5 {
        return Err(wasmtime::Error::msg("raw tree owner layout"));
    }
    let descriptor = owner.field(&mut *store, 0)?;
    let descriptor_ref = descriptor.unwrap_anyref().unwrap();
    let class = classes
        .iter()
        .find_map(|class| {
            match wasmtime::Rooted::ref_eq(&*store, &class.descriptor, descriptor_ref) {
                Ok(true) => Some(Ok(class)),
                Ok(false) => None,
                Err(e) => Some(Err(e)),
            }
        })
        .transpose()?
        .ok_or_else(|| wasmtime::Error::msg("raw tree unknown canonical descriptor"))?;
    if !wasmtime::StructType::eq(&owner.ty(&*store)?, &class.owner_type) {
        return Err(wasmtime::Error::msg("raw tree canonical owner type"));
    }
    for ancestor in path.iter() {
        if wasmtime::Rooted::ref_eq(&*store, ancestor, reference)? {
            return Err(wasmtime::Error::msg("raw tree cycle"));
        }
    }
    path.push(reference.to_owned_rooted(&mut *store)?);
    let result = (|| {
        if class.name == "Error" {
            assert_eq!(count, 5);
            let descriptor = descriptor_ref.as_struct(&*store)?.unwrap();
            assert_eq!(descriptor.fields(&mut *store)?.count(), 5);
            assert_eq!(descriptor.field(&mut *store, 0)?.unwrap_i64(), 7);
            let message = owner.field(&mut *store, 1)?;
            let array = message.unwrap_anyref().unwrap().as_array(&*store)?.unwrap();
            assert!(matches!(
                array.ty(&*store)?.field_type().element_type(),
                wasmtime::StorageType::I16
            ));
            let len = array.len(&*store)?;
            if len > 4096 {
                return Err(wasmtime::Error::msg("raw tree Error message bound"));
            }
            let mut units = Vec::with_capacity(len as usize);
            for i in 0..len {
                units.push(array.get(&mut *store, i)?.unwrap_i32() as u16);
            }
            for i in [2, 3] {
                let field = owner.field(&mut *store, i)?;
                assert_eq!(
                    field
                        .unwrap_anyref()
                        .unwrap()
                        .as_i31(&*store)?
                        .unwrap()
                        .get_u32(),
                    0
                );
            }
            return Ok(json!({"tag":"error","class":"Error","message":units}));
        }
        assert_eq!(count, 4);
        let descriptor_storage = descriptor_ref.as_struct(&*store)?.unwrap();
        assert_eq!(descriptor_storage.fields(&mut *store)?.count(), 5);
        let schema_value = descriptor_storage.field(&mut *store, 1)?;
        let schema = schema_value
            .unwrap_anyref()
            .unwrap()
            .as_array(&*store)?
            .unwrap();
        let expected_fields = if class.name == "Reduced" { 1 } else { 5 };
        assert_eq!(schema.len(&*store)?, expected_fields);
        // Bound every metadata string before source_field collects its units.
        for i in 0..expected_fields {
            let name_value = schema.get(&mut *store, i)?;
            let name = name_value
                .unwrap_anyref()
                .unwrap()
                .as_array(&*store)?
                .unwrap();
            if name.len(&*store)? > 32
                || !matches!(name.ty(&*store)?.element_type(), wasmtime::StorageType::I16)
            {
                return Err(wasmtime::Error::msg("raw tree bounded schema name"));
            }
        }
        if class.name == "Reduced" {
            let mut store = store.as_context_mut();
            let field = source_field!(store, value, &["val"], "val");
            return Ok(
                json!({"tag":"reduced","value":raw_tree_decode(&mut store,&field,classes,path,remaining)?}),
            );
        }
        let mut result = json!({"tag":"node","class":class.name});
        for (source_name, json_name) in [
            ("key", "key"),
            ("val", "val"),
            ("left", "left"),
            ("right", "right"),
            ("__hash", "hash"),
        ] {
            let mut store = store.as_context_mut();
            let field = source_field!(store, value, NODE_FIELDS, source_name);
            let observed = raw_tree_decode(&mut store, &field, classes, path, remaining)?;
            if matches!(source_name, "left" | "right")
                && !matches!(observed["tag"].as_str(), Some("nil" | "node"))
            {
                return Err(wasmtime::Error::msg("raw tree child class"));
            }
            result[json_name] = observed;
        }
        Ok(result)
    })();
    path.pop();
    result
}
#[test]
fn raw_tree_shapes_comparison_bits_errors_and_traces_match_fresh_pinned_observations() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/tree-node-raw-cases.json"
    ))
    .unwrap();
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 52);
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut s = setup(phase);
        s.eval("(def raw-tree-trace 0)").unwrap();
        let mut classes = Vec::new();
        for (name, source) in [
            ("BlackNode", "(BlackNode. 0 0 nil nil nil)"),
            ("RedNode", "(RedNode. 0 0 nil nil nil)"),
            ("Reduced", "(reduced nil)"),
            ("Error", "(suss.bootstrap/error \"anchor\")"),
        ] {
            let anchor = s.eval(source).unwrap();
            let (descriptor, owner_type) = s
                .inspect(&anchor, |mut store, value| {
                    let owner = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                    let descriptor = owner
                        .field(&mut store, 0)?
                        .unwrap_anyref()
                        .unwrap()
                        .to_owned_rooted(&mut store)?;
                    Ok((descriptor, owner.ty(&store)?))
                })
                .unwrap();
            classes.push(RawTreeClass {
                _anchor: anchor,
                descriptor,
                owner_type,
                name,
            });
        }
        // Decoder rejection controls are authored, not yet executed natively.
        let unknown = s.eval("(js-obj)").unwrap();
        assert!(
            s.inspect(&unknown, |mut store, value| raw_tree_decode(
                &mut store,
                &value,
                &classes,
                &mut Vec::new(),
                &mut 4096
            ))
            .is_err()
        );
        let cycle = s.eval("(BlackNode. 1 10 nil nil nil)").unwrap();
        let non_tree_child = s.eval("false").unwrap();
        let false_root = s
            .inspect(&non_tree_child, |mut store, value| {
                value.unwrap_anyref().unwrap().to_owned_rooted(&mut store)
            })
            .unwrap();
        s.inspect(&cycle, |mut store, value| {
            assert!(
                raw_tree_decode(&mut store, &value, &classes, &mut Vec::new(), &mut 0).is_err()
            );
            let owner = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let fields = owner.field(&mut store, 1)?;
            let slots = fields.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            let saved = slots.get(&mut store, 2)?;
            slots.set(&mut store, 2, value.clone())?;
            let rejected =
                raw_tree_decode(&mut store, &value, &classes, &mut Vec::new(), &mut 4096).is_err();
            slots.set(&mut store, 2, saved.clone())?;
            assert!(rejected, "cycle must fail closed");
            let false_value = Val::AnyRef(Some(false_root.to_rooted(&mut store)));
            slots.set(&mut store, 2, false_value)?;
            let rejected =
                raw_tree_decode(&mut store, &value, &classes, &mut Vec::new(), &mut 4096).is_err();
            slots.set(&mut store, 2, saved)?;
            assert!(rejected, "non-tree child must fail closed");
            Ok(())
        })
        .unwrap();
        s.collect().unwrap();
        s.inspect(&cycle, |mut store, value| {
            raw_tree_decode(&mut store, &value, &classes, &mut Vec::new(), &mut 4096)
        })
        .unwrap();
        for case in cases {
            s.eval("(set! raw-tree-trace 0)").unwrap();
            let value = match (
                case["mode"].as_str().unwrap(),
                s.eval(case["source"].as_str().unwrap()),
            ) {
                ("return", Ok(v)) | ("throw", Err(SessionError::Language(v))) => v,
                (_, result) => panic!("{}: unexpected return/throw: {result:?}", case["id"]),
            };
            s.collect().unwrap();
            let actual = s
                .inspect(&value, |mut store, value| {
                    raw_tree_decode(&mut store, &value, &classes, &mut Vec::new(), &mut 4096)
                })
                .unwrap();
            assert_eq!(actual, case["expected"]["value"], "{}", case["id"]);
            let trace = s.eval("raw-tree-trace").unwrap();
            s.collect().unwrap();
            assert_eq!(
                format!("{:016x}", number_bits(&mut s, &trace)),
                case["expected"]["trace"]["bits"].as_str().unwrap(),
                "{} trace",
                case["id"]
            );
            // Recovery is observed directly even after an original thrown value.
            let recovery = s.eval("(+ 17 43)").unwrap();
            assert_eq!(number_bits(&mut s, &recovery), 60.0_f64.to_bits());
        }
    }
}
