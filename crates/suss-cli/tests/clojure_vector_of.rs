//! `vector-of` (#350) against JVM Clojure 1.12.6: a persistent vector whose
//! elements are stored unboxed in packed arrays. Expected strings are JVM
//! Clojure's, except `str`: JVM Clojure prints `clojure.core.Vec@<hash>`,
//! where Suss prints the vector.
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
fn vector_of_behaves_as_jvm_clojure() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        session.eval("(def v (vector-of :long 1 2 3))").unwrap();
        session.eval("(def vl (into (vector-of :long) (range 70)))").unwrap();
        for (source, expected) in [
            ("v", "[1 2 3]"),
            ("(conj v 4)", "[1 2 3 4]"),
            ("(nth v 1)", "2"),
            ("(count (into (vector-of :double) (range 100)))", "100"),
            ("(nth (into (vector-of :double) (range 100)) 70)", "70.0"),
            ("(= v [1 2 3])", "true"),
            ("(= [1 2 3] v)", "true"),
            ("(= (hash v) (hash [1 2 3]))", "true"),
            ("(assoc v 0 9)", "[9 2 3]"),
            ("(assoc v 3 4)", "[1 2 3 4]"),
            ("(pop v)", "[1 2]"),
            ("(peek v)", "3"),
            ("(conj (vector-of :int) 1.5)", "[1]"),
            (
                "(conj (vector-of :byte) 200)",
                r#"[:error "Value out of range for byte: 200"]"#,
            ),
            ("(reduce + (into (vector-of :long) (range 1000)))", "499500"),
            ("(v 2)", "3"),
            ("(seq (vector-of :short 7 8))", "(7 8)"),
            ("(chunked-seq? (seq vl))", "true"),
            ("(count (seq vl))", "70"),
            ("(count (rest (seq vl)))", "69"),
            ("(first (drop 40 (seq vl)))", "40"),
            ("(take 3 (drop 31 (seq vl)))", "(31 32 33)"),
            ("(reduce + (seq vl))", "2415"),
            ("(reduce + (rest (seq vl)))", "2415"),
            ("(reduce + 5 (nth (iterate next (seq vl)) 33))", "1892"),
            ("(= (seq vl) (range 70))", "true"),
            ("(= (hash (seq vl)) (hash (range 70)))", "true"),
            ("(into [] (map inc) (seq (vector-of :double 1 2)))", "[2.0 3.0]"),
            ("(map inc (vector-of :int 1 2 3))", "(2 3 4)"),
            ("(filter even? (vector-of :long 1 2 3 4))", "(2 4)"),
            ("(seq (vector-of :double 1 2))", "(1.0 2.0)"),
            ("(conj (seq (vector-of :long 1 2)) 0)", "(0 1 2)"),
            ("(empty (seq (vector-of :long 1 2)))", "()"),
            ("(count (nth (iterate next (seq vl)) 64))", "6"),
            ("(= (vec (seq vl)) vl)", "true"),
            ("(meta (with-meta (seq vl) {:a 1}))", "{:a 1}"),
            ("(last (seq vl))", "69"),
            ("(nth (iterate next (seq vl)) 70)", "nil"),
            ("(count (chunk-first (seq vl)))", "32"),
            ("(count (chunk-first (nth (iterate next (seq vl)) 64)))", "6"),
            ("(seq (chunk-rest (nth (iterate next (seq vl)) 64)))", "nil"),
            ("(vector-of :foo)", r#"[:error "Unrecognized type :foo"]"#),
            ("(get v 5 :nf)", ":nf"),
            (
                "(pop (vector-of :long))",
                r#"[:error "Can't pop empty vector"]"#,
            ),
            ("(reduce + (vector-of :long 5))", "5"),
            ("(reduce + (vector-of :long))", "0"),
            ("(reduce + 1 (vector-of :long))", "1"),
            (
                "(reduce (fn [a x] (if (= x 40) (reduced a) (+ a x))) (into (vector-of :long) (range 70)))",
                "780",
            ),
            (
                "(reduce conj [] (into (vector-of :int) (range 65)))",
                "[0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 32 33 34 35 36 37 38 39 40 41 42 43 44 45 46 47 48 49 50 51 52 53 54 55 56 57 58 59 60 61 62 63 64]",
            ),
            ("(rseq v)", "(3 2 1)"),
            ("(rseq (vector-of :long))", "nil"),
            ("(compare v (vector-of :long 1 2 4))", "-1"),
            ("(compare v [1 2])", "1"),
            ("(compare (vector-of :int 2) (vector-of :int 1))", "1"),
            (
                "(subvec (into (vector-of :long) (range 40)) 30 33)",
                "[30 31 32]",
            ),
            // Clojure's Vec is not editable: transient throws, as on the JVM.
            (
                "(vector? (try (transient v) (catch :default e :thrown)))",
                "false",
            ),
            // Leaves across a chunk boundary, assoc into a full leaf, and pop
            // back through it.
            (
                "(let [w (into (vector-of :long) (range 70))] [(nth w 31) (nth w 32) (nth (assoc w 5 -5) 5) (count (nth (iterate pop w) 40))])",
                "[31 32 -5 30]",
            ),
        ] {
            assert_eq!(printed(&mut session, source), expected, "{source}");
        }
    }
}
