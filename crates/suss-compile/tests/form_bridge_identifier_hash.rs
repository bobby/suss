//! Cold analyzer identifiers must use the live canonical IHash warming path.
use suss_compile::{
    portable::hir::identifier_hash, portable_macro_data::FormBridge, portable_session::Session,
};

#[test]
fn form_bridge_identifiers_start_cold_and_canonical_hashes_survive_gc() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let bridge = FormBridge::new(&mut session).unwrap();
        let memo = session.eval("(fn [x] (.-_hash x))").unwrap();
        let hash = session.eval("(fn [x] (hash x))").unwrap();
        let hit = session
            .eval("(fn [x h] [(identical? h (.-_hash x)) (= h (hash x)) (identical? h (-hash x))])")
            .unwrap();
        for (keyword, namespace, name, pinned) in [
            (false, None, "memo-symbol-leaf-warm", Some(1_454_146_648)),
            (true, None, "memo-keyword-leaf-warm", Some(-24_028_250)),
            (
                false,
                None,
                "memo-symbol-leaf-warm-nil-aware",
                Some(94_864_730),
            ),
            (
                true,
                None,
                "memo-keyword-leaf-warm-nil-aware",
                Some(43_176_044),
            ),
            // These supplement the pinned values with consistency against the
            // retained compiled-constant hash arithmetic; not new oracle cases.
            (false, Some("demo.ns"), "leaf-λ🚀", None),
            (true, Some("demo.ns"), "leaf-λ🚀", None),
        ] {
            let value = bridge
                .identifier(&mut session, namespace, name, keyword, None)
                .unwrap();
            let cold = session.invoke(&memo, &[&value]).unwrap();
            assert_eq!(
                suss_compile::portable_repl::display(&mut session, &cold).unwrap(),
                "nil"
            );
            session.collect().unwrap();
            let warmed = session.invoke(&hash, &[&value]).unwrap();
            let expected = pinned.unwrap_or_else(|| identifier_hash(namespace, name, keyword));
            let actual = session
                .inspect(&warmed, |mut store, value| {
                    let number = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                    Ok(number.field(&mut store, 0)?.unwrap_f64())
                })
                .unwrap();
            assert_eq!(
                actual,
                f64::from(expected),
                "{namespace:?}/{name}/keyword={keyword}"
            );
            session.collect().unwrap();
            let result = session.invoke(&hit, &[&value, &warmed]).unwrap();
            assert_eq!(
                suss_compile::portable_repl::display(&mut session, &result).unwrap(),
                "[true true true]"
            );
        }
    }
}
