//! Invalid repeat counts follow Clojure: non-number counts throw (#286).
use suss_cli::portable_session::Session;

#[test]
fn repeat_counts_use_nominal_conversion_hooks_and_preserve_source_effects() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        for source in [
            // Clojure governs (#286): test-repeat's :default branch throws for a
            // non-number count, where pinned ClojureScript converts it to NaN.
            "(try (repeat :a :a) false (catch :default e true))",
            "(try (repeat 'not-a-count 7) false (catch :default e true))",
            "(try (repeat [1] 7) false (catch :default e true))",
            "(try (repeat nil nil) false (catch :default e true))",
            // The inherited Object.prototype valueOf returns the exact
            // nominal receiver, and a declared own field still shadows it.
            "(do (deftype RepeatInheritedValue [] Object (toString [this] \"not-a-count\")) (let [instance (RepeatInheritedValue.)] (identical? instance (.valueOf instance))))",
            // Object counts are rejected without running conversion hooks.
            "(do (def repeat-coercion-trace []) (deftype RepeatStringCount [] Object (toString [this] (set! repeat-coercion-trace (conj repeat-coercion-trace :string)) \"not-a-count\")) (and (try (repeat (RepeatStringCount.) 7) false (catch :default e true)) (= repeat-coercion-trace [])))",
            "(do (set! repeat-coercion-trace []) (deftype RepeatNumberCount [] Object (valueOf [this] (set! repeat-coercion-trace (conj repeat-coercion-trace :number)) 0) (toString [this] (throw 99))) (and (try (repeat (RepeatNumberCount.) 7) false (catch :default e (not= e 99))) (= repeat-coercion-trace [])))",
            // Numeric counts, including non-positive ones.
            "(= [7 7] (repeat 2 7))",
            "(= [] (repeat 0 7) (repeat -1 7) (repeat ##NaN 7))",
        ] {
            let value = session.eval(source).unwrap_or_else(|error| panic!("{source}: {error:?}"));
            session.collect().unwrap();
            session.inspect(&value, |store, value| {
                assert_eq!(value.unwrap_anyref().unwrap().as_i31(&store)?.expect("Boolean ABI").get_u32(), 4, "{source}");
                Ok(())
            }).unwrap();
        }
    }
}
