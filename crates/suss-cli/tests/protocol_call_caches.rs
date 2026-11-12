//! Protocol method lookups cache the last descriptor's method per call site
//! (#355). Extending a protocol to a type after a site has dispatched on it
//! takes effect there: every method table write bumps the protocol epoch.
use suss_cli::portable_session::Session;

fn printed(session: &mut Session, source: &str) -> String {
    let value = session
        .eval(&format!("(pr-str {source})"))
        .unwrap_or_else(|error| panic!("{source}: {error}"));
    let units = session
        .inspect(&value, |mut store, value| {
            let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            Ok(array
                .elems(&mut store)?
                .map(|unit| unit.unwrap_i32() as u16)
                .collect::<Vec<_>>())
        })
        .unwrap();
    String::from_utf16(&units).unwrap()
}

#[test]
fn extending_after_a_cached_dispatch_takes_effect_at_that_site() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session
            .eval("(deftype Ix [] IIndexed (-nth [_ i] [:first i]) (-nth [_ i nf] [:first i nf]))")
            .unwrap();
        session
            .eval(
                "(deftype Other [] IIndexed (-nth [_ i] [:other i]) (-nth [_ i nf] [:other i nf]))",
            )
            .unwrap();
        // core's nth dispatches -nth through one cached site.
        assert_eq!(
            printed(&mut session, "[(nth (Ix.) 0) (nth (Ix.) 1)]"),
            "[[:first 0] [:first 1]]"
        );
        // Another type at the same site misses and refills the cache.
        assert_eq!(
            printed(&mut session, "[(nth (Other.) 2) (nth (Ix.) 3)]"),
            "[[:other 2] [:first 3]]"
        );
        session
            .eval("(extend-type Ix IIndexed (-nth ([_ i] [:second i]) ([_ i nf] [:second i nf])))")
            .unwrap();
        assert_eq!(printed(&mut session, "(nth (Ix.) 4)"), "[:second 4]");
        assert_eq!(printed(&mut session, "(nth [10 20] 1)"), "20");
    }
}

/// `implements?` caches its answer per site in the same way (#361).
#[test]
fn extending_after_a_cached_marker_test_takes_effect_at_that_site() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval("(defprotocol P (p [x]))").unwrap();
        session
            .eval("(deftype A []) (deftype B [] P (p [_] :b))")
            .unwrap();
        session
            .eval("(def probe (fn [x] (implements? P x)))")
            .unwrap();
        assert_eq!(
            printed(
                &mut session,
                "[(probe (A.)) (probe (A.)) (probe (B.)) (probe 1) (probe nil)]"
            ),
            "[false false true false false]"
        );
        session.eval("(extend-type A P (p [_] :a))").unwrap();
        assert_eq!(
            printed(&mut session, "[(probe (A.)) (probe (B.)) (p (A.))]"),
            "[true true :a]"
        );
    }
}
