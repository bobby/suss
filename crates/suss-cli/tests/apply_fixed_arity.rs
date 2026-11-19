//! `apply` calls a fn with fixed arities only through an ordinary call, with
//! up to twenty arguments counted as Clojure's applyToHelper counts them
//! (#466); a variadic fn's rest is the same IndexedSeq as before.
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

#[test]
fn apply_over_seqs_answers_as_before() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session
            .eval(
                "(def f2 (fn [a b] [a b]))
                 (def f0-3 (fn ([] :zero) ([a] [a]) ([a b] [a b]) ([a b c] [a b c])))
                 (def f20 (fn [a b c d e f g h i j k l m n o p q r s t] [a t]))
                 (def fv (fn [a & more] [a more]))
                 (def outcome (fn [f] (try (f) (catch :default e :threw))))",
            )
            .unwrap();
        for (source, expected) in [
            ("[(apply f2 1 (list 2)) (apply f2 (list 1 2)) (apply f2 (seq [1 2])) (apply f2 1 (range 2 3))]", "[[1 2] [1 2] [1 2] [1 2]]"),
            ("[(apply f0-3 ()) (apply f0-3 nil) (apply f0-3 (list 1)) (apply f0-3 1 2 (list 3)) (apply f0-3 1 2 3 ())]", "[:zero :zero [1] [1 2 3] [1 2 3]]"),
            ("(apply f20 (range 20))", "[0 19]"),
            ("(apply f20 0 1 2 3 (range 4 20))", "[0 19]"),
            // Wrong arities and too many arguments throw, finite or not.
            ("[(outcome #(apply f2 (list 1))) (outcome #(apply f2 (list 1 2 3))) (outcome #(apply f2 (range 30)))]", "[:threw :threw :threw]"),
            // More than twenty take apply-to-simple, which reaches fns of more
            // fixed parameters (portable_source_apply.rs).
            // Callables that aren't fns, and variadic fns, as before.
            ("[(apply :a (list {:a 1})) (apply {:a 1} (list :a)) (apply #{3} (list 3)) (apply str (list 1 2 3))]", "[1 1 3 \"123\"]"),
            ("[(apply fv (list 1)) (apply fv 1 (list 2 3)) (apply + (range 10)) (apply + 1 2 (range 3))]", "[[1 nil] [1 (2 3)] 45 6]"),
            ("(first (apply (fn [& xs] xs) (iterate inc 0)))", "0"),
            // A variadic call's rest is an IndexedSeq over its arguments.
            ("(let [xs ((fn [& xs] xs) 1 2 3)] [xs (count xs) (rest xs) (list? xs) (seq? xs)])", "[(1 2 3) 3 (2 3) false true]"),
            ("[(list 1) (list 1 2 3) (list? (list 1 2)) (count (list 1 2 3))]", "[(1) (1 2 3) true 3]"),
        ] {
            assert_eq!(printed(&mut session, source), expected, "{source}");
        }
    }
}
