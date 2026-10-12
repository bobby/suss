//! Invalid repeat counts follow pinned ClojureScript numeric conversion.
use suss_cli::portable_session::Session;

#[test]
fn repeat_counts_use_nominal_conversion_hooks_and_preserve_source_effects() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        for source in [
            // Vendored test-repeat#15: a keyword converts to NaN, not an error.
            "(= [] (repeat :a :a))",
            "(= [] (repeat 'not-a-count 7))",
            "(= [] (repeat [1] 7))",
            // The inherited Object.prototype valueOf returns the exact
            // nominal receiver, and a declared own field still shadows it.
            "(do (deftype RepeatInheritedValue [] Object (toString [this] \"not-a-count\")) (let [instance (RepeatInheritedValue.)] (identical? instance (.valueOf instance))))",
            "(do (def repeat-own-value-field-trace []) (deftype RepeatOwnValueField [valueOf] Object (toString [this] (set! repeat-own-value-field-trace (conj repeat-own-value-field-trace :string)) \"not-a-count\")) (and (= [] (repeat (RepeatOwnValueField. 7) 7)) (= repeat-own-value-field-trace [:string])))",
            "(do (def repeat-coercion-trace []) (deftype RepeatStringCount [] Object (toString [this] (set! repeat-coercion-trace (conj repeat-coercion-trace :string)) \"not-a-count\")) (and (= [] (repeat (RepeatStringCount.) 7)) (= repeat-coercion-trace [:string])))",
            "(do (set! repeat-coercion-trace []) (deftype RepeatNumberCount [] Object (valueOf [this] (set! repeat-coercion-trace (conj repeat-coercion-trace :number)) 0) (toString [this] (throw 99))) (and (= [] (repeat (RepeatNumberCount.) 7)) (= repeat-coercion-trace [:number])))",
            "(do (set! repeat-coercion-trace []) (deftype RepeatThrowingCount [] Object (valueOf [this] (set! repeat-coercion-trace (conj repeat-coercion-trace :number)) (throw 17)) (toString [this] (set! repeat-coercion-trace (conj repeat-coercion-trace :string)) \"0\")) (try (repeat (RepeatThrowingCount.) 7) false (catch :default payload (and (= payload 17) (= repeat-coercion-trace [:number])))))",
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
