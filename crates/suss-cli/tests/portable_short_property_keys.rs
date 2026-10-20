use suss_cli::portable_session::{Session, SessionError};
fn number(s: &mut Session, source: &str) -> f64 {
    let v = s.eval(source).unwrap();
    s.inspect(&v, |mut store, v| {
        store.gc(None)?;
        let o = v.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
        Ok(o.field(&mut store, 0)?.unwrap_f64())
    })
    .unwrap()
}
#[test]
fn short_static_key_preserves_dynamic_receiver_getter_replacement_and_order_after_gc() {
    for mut s in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        s.eval(r#"(def short-events 0)
      (def read-short (fn [o] (.-ns o)))
      (def short-object (js-obj))
      (.__defineGetter__ short-object "ns" (fn [] (set! short-events (+ (* short-events 10) 2)) 17))"#).unwrap();
        assert_eq!(number(&mut s,"(let [answer (read-short (do (set! short-events 1) short-object))] (+ (* answer 100) short-events))"),1712.0);
        assert_eq!(
            number(
                &mut s,
                r#"(do (.__defineGetter__ short-object "ns" (fn [] (set! short-events (+ (* short-events 10) 3)) 19)) (let [answer (read-short (do (set! short-events 1) short-object))] (+ (* answer 100) short-events)))"#
            ),
            1913.0
        );
        // The same compiled unknown-receiver function also takes the nominal own
        // field path. No nominal class or slot was assumed by its caller.
        assert_eq!(
        number(
            &mut s,
            "(do (deftype ShortLiteralFields [ns name]) (read-short (ShortLiteralFields. 23 29)))"
        ),
        23.0
    );
        let e = s.eval("(read-short nil)").unwrap_err();
        assert!(matches!(e, SessionError::Language(_)), "{e:?}");
        assert_eq!(
            number(&mut s, "(read-short (ShortLiteralFields. 31 37))"),
            31.0
        );
    }
}
#[test]
fn short_static_getter_throw_is_not_retried_and_returned_key_alias_stays_live() {
    for mut s in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        s.eval(r#"(def short-effects 0) (def short-thrower (js-obj))
      (.__defineGetter__ short-thrower "ns" (fn [] (set! short-effects (+ short-effects 1)) (throw 91)))"#).unwrap();
        let e = s.eval("(.-ns short-thrower)").unwrap_err();
        let SessionError::Language(v) = e else {
            panic!("expected language throw: {e:?}")
        };
        let thrown = s
            .inspect(&v, |mut store, v| {
                store.gc(None)?;
                let o = v.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                Ok(o.field(&mut store, 0)?.unwrap_f64())
            })
            .unwrap();
        assert_eq!(thrown, 91.0);
        assert_eq!(number(&mut s, "short-effects"), 1.0);
        assert_eq!(
            number(
                &mut s,
                r#"(let [k "ns" o (js-obj)] (unchecked-set o k 41) (let [answer (unchecked-get o k) f (fn [] k)] (if (= (f) "ns") answer 0)))"#
            ),
            41.0
        );
    }
}
