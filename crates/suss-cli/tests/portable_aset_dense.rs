//! `(aset a i v)` on Clojure's arrays (#425): it writes an element inside the
//! array and returns the value; anything else throws.
use suss_cli::portable_session::Session;

fn printed(session: &mut Session, source: &str) -> String {
    let value = session
        .eval(&format!(
            "(pr-str (try {source} (catch :default e [:error (ex-message e)])))"
        ))
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
fn aset_assigns_inside_the_array_and_returns_the_value() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        for (source, expected) in [
            // A present slot, and make-array's nil fill.
            ("(let [a (array 1 2 3)] [(aset a 0 9) (vec a)])", "[9 [9 2 3]]"),
            ("(let [n (count [1 2 3]) a (make-array n)] (aset a 1 :x) [(vec a) (alength a) (nil? (aget a 0)) (nil? (aget a 1))])", "[[nil :x nil] 3 true false]"),
            // Arrays do not grow (#425): an index past the end throws.
            ("(let [a (array 1)] [(try (aset a 3 :z) (catch :default e :threw)) (alength a) (vec a)])", "[:threw 1 [1]]"),
            // A double index truncates, as on the JVM.
            ("(let [a (array 1 2)] (aset a 1.0 :d) (vec a))", "[1 :d]"),
            // Packed arrays convert; nested indices; host objects.
            ("(let [a (long-array 2)] (aset a 1 5) (vec a))", "[0 5]"),
            ("(let [a (int-array 1)] (aset a 0 1.5) (vec a))", "[1]"),
            ("(let [a (array (array 1 2))] (aset a 0 1 :n) (aget a 0 1))", ":n"),
            (r#"(let [o (js-obj)] (aset o "k" 1) (aget o "k"))"#, r#"[:error "Invalid or unsupported array operation"]"#),
            ("(aset nil 0 1)", r#"[:error "Invalid or unsupported array operation"]"#),
            ("(let [t (transient [])] (count (persistent! (reduce conj! t (range 70)))))", "70"),
        ] {
            assert_eq!(printed(&mut session, source), expected, "{source}");
        }
    }
}
