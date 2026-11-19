//! Core's numeric coercions compile inline as raw results (#448): `long`,
//! `double`, `unchecked-long` and `unchecked-double` answer and throw as a
//! call of the var does, for raw and boxed longs and doubles and other
//! values. Ratios and BigInts have no literals yet (#338, #339).
use suss_cli::portable_session::Session;

fn printed(session: &mut Session, source: &str) -> String {
    let value = session
        .eval(&format!("(pr-str {source})"))
        .unwrap_or_else(|error| panic!("{source}: {error}"));
    let units = session
        .inspect(&value, |mut store, value| {
            let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            Ok(array.elems(&mut store)?.map(|unit| unit.unwrap_i32() as u16).collect::<Vec<_>>())
        })
        .unwrap();
    String::from_utf16(&units).unwrap()
}

const VALUES: &str = "[0 1 -1 1073741823 1073741824 -1073741825 9223372036854775807 -9223372036854775808
  1.5 -1.5 2.5 -0.0 0.0 1.0e30 -1.0e30 9.2233720368547758E18 ##NaN ##Inf ##-Inf
  nil \"x\" :k [] (atom 1)]";

#[test]
fn coercions_answer_as_calls_do() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session
            .eval(
                "(def outcome (fn [f] (try (f) (catch :default e :threw))))
                 (def inline (fn [x] [(outcome (fn [] (long x))) (outcome (fn [] (double x)))
                                      (outcome (fn [] (unchecked-long x))) (outcome (fn [] (unchecked-double x)))]))
                 (def called (fn [x] [(outcome (fn [] (apply long [x]))) (outcome (fn [] (apply double [x])))
                                      (outcome (fn [] (apply unchecked-long [x]))) (outcome (fn [] (apply unchecked-double [x])))]))",
            )
            .unwrap();
        let inline = printed(&mut session, &format!("(mapv inline {VALUES})"));
        let called = printed(&mut session, &format!("(mapv called {VALUES})"));
        assert_eq!(inline, called);
        // Spot checks against Clojure's answers.
        for (source, expected) in [
            ("(long 1.5)", "1"),
            ("(long -1.5)", "-1"),
            ("(double 3)", "3.0"),
            ("(double -0.0)", "-0.0"),
            ("(unchecked-long 1.0e30)", "9223372036854775807"),
            ("(unchecked-long ##NaN)", "0"),
            ("(try (long 1.0e30) (catch :default e :threw))", ":threw"),
            ("(try (long nil) (catch :default e :threw))", ":threw"),
            ("(try (double \"x\") (catch :default e :threw))", ":threw"),
        ] {
            assert_eq!(printed(&mut session, source), expected, "{source}");
        }
    }
}

#[test]
fn raw_operands_coerce_inline() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        for (source, expected) in [
            // A raw long counter widened, added and truncated.
            ("(loop [i 0 acc 0.0] (if (< i 1000) (recur (inc i) (+ acc (double i) 0.5)) acc))", "500000.0"),
            ("(loop [i 0 acc 0] (if (< i 100) (recur (inc i) (+ acc (unchecked-long (+ (double i) 0.75)))) acc))", "4950"),
            ("(loop [i 0 acc 0] (if (< i 100) (recur (inc i) (+ acc (long i))) acc))", "4950"),
            // A raw double out of a long's range still throws when checked.
            ("(try (loop [d 1.0 n 0] (if (< n 70) (recur (* d 2.0) (inc n)) (long d))) (catch :default e :threw))", ":threw"),
            ("(loop [d 1.0 n 0] (if (< n 70) (recur (* d 2.0) (inc n)) (unchecked-long d)))", "9223372036854775807"),
            // Past the i31 range, and the overflow checks around it.
            ("(let [x (long 1073741824)] [(* x 2) (inc x) (double x)])", "[2147483648 1073741825 1.073741824E9]"),
            ("(try (inc (long 9223372036854775807)) (catch :default e :threw))", ":threw"),
        ] {
            assert_eq!(printed(&mut session, source), expected, "{source}");
        }
    }
}
