use suss_compile::{portable_macro_data::FormBridge, portable_session::Session};
use suss_reader::forms::Kind;

#[test]
fn primitive_string_access_keeps_utf16_and_numeric_index_boundaries() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let value = session.eval(r#"(let [s "A\uD83D\uDE00\uD800"]
          [(.-length "") (.-length s)
           (.charCodeAt s 0) (.charCodeAt s 1) (.charCodeAt s 2) (.charCodeAt s 3)
           (.charCodeAt s -0.9) (.charCodeAt s 1.9) (.charCodeAt s ##NaN)
           (.charCodeAt s -1) (.charCodeAt s 4) (.charCodeAt s ##Inf)
           (.charCodeAt s ##-Inf) (.charCodeAt s 1e20)
           (.charCodeAt s "1") (.charCodeAt s true) (.charCodeAt s nil)])"#).unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let form = bridge.read(&mut session, &value, 0..1).unwrap();
        let Kind::Vector(values) = &form.kind else { panic!("expected vector") };
        let expected = [0.,4.,65.,55357.,56832.,55296.,65.,55357.,65.,
            f64::NAN,f64::NAN,f64::NAN,f64::NAN,f64::NAN,55357.,55357.,65.];
        assert_eq!(values.len(), expected.len());
        for (value, expected) in values.iter().zip(expected) {
            let Kind::Number(actual) = value.kind else { panic!("expected number: {value:?}") };
            assert!(actual == expected || (actual.is_nan() && expected.is_nan()), "{actual} != {expected}");
        }
    }
}

#[test]
fn primitive_string_access_evaluates_once_and_keeps_object_coercion_dispatch_and_errors() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let value = session.eval(r#"(def access-events [])
          (def access-index (js-obj))
          (suss.bootstrap/object-set-strict access-index "valueOf"
            (fn [] (do (set! access-events (conj access-events 5)) 1)))
          (def access-owner (js-obj))
          (suss.bootstrap/object-set-strict access-owner "length" 17)
          (suss.bootstrap/object-set-strict access-owner "charCodeAt"
            (fn [index] (do (set! access-events (conj access-events 6)) (+ 100 index))))
          [(.-length (do (set! access-events (conj access-events 1)) "AB"))
           (.charCodeAt (do (set! access-events (conj access-events 2)) "AB")
             (do (set! access-events (conj access-events 3)) 1))
           (.charCodeAt "AB" (do (set! access-events (conj access-events 4)) access-index))
           (.-length access-owner) (.charCodeAt access-owner 7)
           (.-length (array 1 2 3))
           (try (let [owner nil] (.-length owner)) (catch :default error true))
           (try (.charCodeAt nil 0) (catch :default error true))
           (try (.charCodeAt "AB") (catch :default error true))
           (try (.charCodeAt "AB" 0 1) (catch :default error true))
           (try (let [f (.-charCodeAt "AB")] (f 0)) (catch :default error true))
           access-events]"#).unwrap();
        let display = suss_compile::portable_repl::display(&mut session, &value).unwrap();
        assert_eq!(display, "[2 66 66 17 107 3 true true 65 65 true [1 2 3 4 5 6]]");
    }
}
