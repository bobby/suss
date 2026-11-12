//! `(aset a i v)` writes a source array's dense prefix directly (#350); every
//! other case keeps `source-array-set-or-property`'s results. The expected
//! values are the full assignment's, observed before the direct write existed.
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
fn aset_keeps_source_array_assignment_results() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        for (source, expected) in [
            // Dense: a present slot, and a hole of make-array inside the length.
            ("(let [a (array 1 2 3)] [(aset a 0 9) (vec a)])", "[9 [9 2 3]]"),
            ("(let [n (count [1 2 3]) a (make-array n)] (aset a 1 :x) [(vec a) (alength a) (undefined? (aget a 0)) (undefined? (aget a 1))])", "[[nil :x nil] 3 true false]"),
            // At and past the length, beyond the dense prefix, and growth.
            ("(let [a (array 1)] (aset a 1 :z) [(alength a) (vec a)])", "[2 [1 :z]]"),
            ("(let [a (array 1)] (aset a 3 :z) [(alength a) (vec a) (undefined? (aget a 2))])", "[4 [1 nil nil :z] true]"),
            ("(let [a (array 1 2 3)] (aset a 7 :far) (aset a 5 :mid) [(alength a) (aget a 5) (aget a 6) (aget a 7)])", "[8 :mid nil :far]"),
            ("(let [a (array)] (loop [i 0] (when (< i 40) (aset a i i) (recur (inc i)))) [(alength a) (aget a 39) (reduce + (vec a))])", "[40 39 780]"),
            // Negative, double and string keys.
            ("(let [a (array 1)] [(aset a -1 :n) (alength a) (vec a)])", "[:n 1 [1]]"),
            ("(let [a (array 1 2)] (aset a 1.0 :d) (vec a))", "[1 :d]"),
            (r#"(let [a (array 1 2)] (aset a "1" :s) (vec a))"#, "[1 :s]"),
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
