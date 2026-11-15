//! Arithmetic rejects non-numbers without converting them, in both caller phases.
use suss_cli::portable_session::Session;
#[test]
fn arithmetic_rejects_objects_without_converting_them() {
    // Clojure arithmetic, comparison and bit operations reject objects before
    // converting them (#286, #336).
    let cases = [
        "(let [trace (atom []) x (reify Object (toString [_] (swap! trace conj :x) \"2\"))] (and (= \"bit operation not supported for a non-number\" (try (bit-or x 0) nil (catch :default e (ex-message e)))) (= @trace [])))",
        "(let [trace (atom []) x (reify Object (toString [_] (swap! trace conj :x) \"2\")) y (reify Object (toString [_] (swap! trace conj :y) \"3\"))] (and (= \"bit operation not supported for a non-number\" (try (bit-or x y) nil (catch :default e (ex-message e)))) (= @trace [])))",
        "(let [trace (atom []) x (reify Object (toString [_] (swap! trace conj :x) \"2\"))] (and (= \"< requires number operands\" (try (< x 3) nil (catch :default e (ex-message e)))) (= @trace [])))",
        "(let [trace (atom []) x (reify Object (toString [_] (swap! trace conj :x) \"2\"))] (and (= \"+ requires number operands\" (try (+ x 3) nil (catch :default e (ex-message e)))) (= @trace [])))",
    ];
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        for source in cases {
            let value = session
                .eval(source)
                .unwrap_or_else(|error| panic!("{source}: {error}"));
            session.collect().unwrap();
            session
                .inspect(&value, |mut store, value| {
                    assert_eq!(
                        suss_cli::portable_session::sentinel_tag(&mut store, &value)?.expect("Boolean ABI"),
                        4,
                        "{source}"
                    );
                    Ok(())
                })
                .unwrap();
        }
    }
}

