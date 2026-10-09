//! Execute object conversion order through public source, in both caller phases.
use suss_cli::portable_session::Session;
#[test]
fn ordinary_object_coercion_preserves_hints_order_and_primitive_results() {
    let cases = [
        "(< (js-obj \"valueOf\" (fn [] 2)) 3)",
        "(not (< (js-obj \"valueOf\" (fn [] \"2\")) \"10\"))",
        "(let [trace (atom []) x (js-obj \"valueOf\" (fn [] (swap! trace conj :x) 2)) y (js-obj \"valueOf\" (fn [] (swap! trace conj :y) 3))] (and (< x y) (= @trace [:x :y])))",
        "(let [trace (atom []) a (array) key (js-obj \"toString\" (fn [] (swap! trace conj :string) \"label\") \"valueOf\" (fn [] (swap! trace conj :number) 7))] (aset a key 23) (and (= (aget a \"label\") 23) (= @trace [:string])))",
        "(let [trace (atom []) x (js-obj \"valueOf\" (fn [] (swap! trace conj :number) (js-obj)) \"toString\" (fn [] (swap! trace conj :string) \"2\"))] (and (< x 3) (= @trace [:number :string])))",
        "(let [a (array) key (js-obj \"toString\" 17 \"valueOf\" (fn [] 7))] (aset a key 23) (= (aget a 7) 23))",
        "(= (+ (js-obj \"valueOf\" (fn [] \"2\")) 3) \"23\")",
        "(let [trace (atom []) x (js-obj \"valueOf\" (fn [] (swap! trace conj :x) (throw 17))) y (js-obj \"valueOf\" (fn [] (swap! trace conj :y) 3))] (try (< x y) false (catch :default e (and (= e 17) (= @trace [:x])))))",
    ];
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        for source in cases {
            let value = session
                .eval(source)
                .unwrap_or_else(|error| panic!("{source}: {error}"));
            session.collect().unwrap();
            session
                .inspect(&value, |store, value| {
                    assert_eq!(
                        value
                            .unwrap_anyref()
                            .unwrap()
                            .as_i31(&store)?
                            .expect("Boolean ABI")
                            .get_u32(),
                        4,
                        "{source}"
                    );
                    Ok(())
                })
                .unwrap();
        }
    }
}

#[test]
fn array_length_assignment_converts_object_rhs_twice_in_order() {
    let cases = [
        "(let [n (atom 0) a (array 7 8 9) rhs (js-obj \"valueOf\" (fn [] (swap! n inc)))] (try (aset a \"length\" rhs) false (catch :default e (and (= (.-message e) \"Invalid array length\") (= @n 2) (= (alength a) 3)))))",
        "(let [n (atom 0) a (array 7 8 9) rhs (js-obj \"valueOf\" (fn [] (if (= (swap! n inc) 1) 1 (throw 17))))] (try (aset a \"length\" rhs) false (catch :default e (and (= e 17) (= @n 2) (= (alength a) 3)))))",
        "(let [n (atom 0) a (array 7 8 9) rhs (js-obj \"valueOf\" (fn [] (swap! n inc) (if (= @n 1) (do (.push a 23) 1) 1)))] (aset a \"length\" rhs) (and (= @n 2) (= (alength a) 1) (= (aget a 0) 7) (not (.hasOwnProperty a \"3\"))))",
        "(let [n (atom 0) a (array 7 8 9) rhs (js-obj \"valueOf\" (fn [] (if (= (swap! n inc) 1) 4294967297 1)))] (aset a \"length\" rhs) (and (= @n 2) (= (alength a) 1)))",
    ];
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        for source in cases {
            let value = session
                .eval(source)
                .unwrap_or_else(|error| panic!("{source}: {error}"));
            session.collect().unwrap();
            session
                .inspect(&value, |store, value| {
                    assert_eq!(
                        value
                            .unwrap_anyref()
                            .unwrap()
                            .as_i31(&store)?
                            .expect("Boolean ABI")
                            .get_u32(),
                        4,
                        "{source}"
                    );
                    Ok(())
                })
                .unwrap();
        }
    }
}
