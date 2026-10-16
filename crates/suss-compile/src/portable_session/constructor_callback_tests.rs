//! Supplemental construction effects observed at pinned CLJS c4295f303100bbf5afac449242d30bca1126f1a1.
//! Oracle: m4-namespace-constructor-callback-oracle-2026-10-16.tar.gz.
//! These are ordinary guest parents. Immutable source retransport and certificate
//! publication require separate source-root tests; this test supplies no authority.
//! The no-construction control does not execute a macro environment handoff.
use super::{Session, memo_leaf_test_support::Method};
use crate::{portable::hir::Literal, portable_macro_data::FormBridge};
use suss_reader::forms::Kind;

#[test]
fn bridge_construction_preserves_selected_hash_callback_order_and_earlier_guest_poison() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval("(def leaf-original-method nil) (def constructor-subject nil) (def constructor-parent nil) (def constructor-events [])").unwrap();
        let original = Method::capture(&mut session, "PersistentArrayMap").unwrap();
        original.bind_delegate(&mut session).unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        session
            .eval(
                r#"(extend-type PersistentArrayMap IHash
          (-hash [self]
            (if (identical? self constructor-subject)
              (do
                (set! constructor-events (conj constructor-events "enter"))
                (aset (.-arr constructor-parent) 1 "callback-poison")
                (set! constructor-events (conj constructor-events "poison"))
                (let [result (leaf-original-method self)]
                  (set! constructor-events (conj constructor-events result))
                  result))
              (leaf-original-method self))))"#,
            )
            .unwrap();
        // Retain an error, restore the actual selected method, then assert. A
        // failed boundary must not silently leave a replaced protocol installed.
        let result: Result<(), String> = (|| {
            for route in [
                "no-construction",
                "map-small",
                "map-large",
                "set-small",
                "set-large",
            ] {
                session.eval("(def constructor-subject {:token 41}) (def constructor-parent {:doc \"original-doc\"}) (set! constructor-events [])")
                    .map_err(|e| format!("{route} setup: {e:?}"))?;
                let subject = session
                    .eval("constructor-subject")
                    .map_err(|e| format!("subject: {e:?}"))?;
                let cold = session
                    .eval("(nil? (.-__hash constructor-subject))")
                    .map_err(|e| format!("cold control: {e:?}"))?;
                let cold = bridge
                    .read(&mut session, &cold, 0..0)
                    .map_err(|e| format!("decode cold: {e:?}"))?;
                if cold.kind != Kind::Bool(true) {
                    return Err(format!("{route}: subject did not start cold"));
                }
                let mut keys = vec![subject];
                if route.ends_with("large") {
                    for name in [":a", ":b", ":c", ":d", ":e", ":f", ":g", ":h"] {
                        keys.push(session.eval(name).map_err(|e| format!("key: {e:?}"))?);
                    }
                }
                let value = bridge
                    .scalar(&mut session, &Literal::Number(17.0))
                    .map_err(|e| format!("scalar: {e:?}"))?;
                let built = if route.starts_with("map-") {
                    let entries = keys
                        .iter()
                        .map(|key| (key.clone(), value.clone()))
                        .collect::<Vec<_>>();
                    Some(
                        bridge
                            .map_values(&mut session, &entries)
                            .map_err(|e| format!("{route}: {e:?}"))?,
                    )
                } else if route.starts_with("set-") {
                    Some(
                        bridge
                            .set_values(&mut session, &keys)
                            .map_err(|e| format!("{route}: {e:?}"))?,
                    )
                } else {
                    None
                };
                session.collect().map_err(|e| format!("GC: {e:?}"))?;
                if let Some(built) = built {
                    let inspect = session.eval("(fn [coll key] [(count coll) (boolean (some (fn [x] (identical? x key)) (if (map? coll) (keys coll) coll)))])").map_err(|e| format!("observer: {e:?}"))?;
                    let observed = session
                        .invoke(&inspect, &[&built, &keys[0]])
                        .map_err(|e| format!("observe built: {e:?}"))?;
                    let observed = bridge
                        .read(&mut session, &observed, 0..0)
                        .map_err(|e| format!("decode built: {e:?}"))?;
                    let Kind::Vector(items) = &observed.kind else {
                        return Err(format!("{route}: non-vector observation"));
                    };
                    if !matches!(items.as_slice(), [count, exact] if count.kind == Kind::Number(keys.len() as f64) && exact.kind == Kind::Bool(true))
                    {
                        return Err(format!(
                            "{route}: construction lost count or exact subject: {items:?}"
                        ));
                    }
                }
                let observed = session
                    .eval("[constructor-events (:doc constructor-parent)]")
                    .map_err(|e| format!("{route} observation: {e:?}"))?;
                let observed = bridge
                    .read(&mut session, &observed, 0..0)
                    .map_err(|e| format!("{route} decode: {e:?}"))?;
                let expected = if route.ends_with("large") {
                    "[[\"enter\" \"poison\" 124169561] \"callback-poison\"]"
                } else {
                    "[[] \"original-doc\"]"
                };
                let expected = suss_reader::forms::read_forms(expected).unwrap().remove(0);
                // Reader spans differ, so compare the lossless projected kinds.
                fn equivalent(a: &Kind, b: &Kind) -> bool {
                    match (a, b) {
                        (Kind::Vector(a), Kind::Vector(b)) => {
                            a.len() == b.len()
                                && a.iter().zip(b).all(|(a, b)| equivalent(&a.kind, &b.kind))
                        }
                        _ => a == b,
                    }
                }
                if !equivalent(&observed.kind, &expected.kind) {
                    return Err(format!("{route}: {observed:?}; expected {expected:?}"));
                }
                // Cleanup of this ordinary guest object is explicit test work;
                // source certificates must never repair an old guest this way.
                session
                    .eval("(aset (.-arr constructor-parent) 1 \"original-doc\")")
                    .map_err(|e| format!("cleanup: {e:?}"))?;
            }
            Ok(())
        })();
        original.restore(&mut session).unwrap();
        assert!(
            original
                .is_current(&mut session, "PersistentArrayMap")
                .unwrap(),
            "cleanup must restore the actual selected method"
        );
        assert!(result.is_ok(), "{}", result.unwrap_err());
    }
}
