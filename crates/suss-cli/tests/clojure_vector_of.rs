//! `vector-of` (#350) against JVM Clojure 1.12.6: a persistent vector whose
//! elements are stored unboxed in packed arrays. Expected strings are JVM
//! Clojure's, except `str`: JVM Clojure prints `clojure.core.Vec@<hash>`,
//! where Suss prints the vector.
use suss_cli::portable_session::Session;

fn printed(session: &mut Session, source: &str) -> String {
    let value = session
        .eval(&format!("(pr-str (try {source} (catch :default e [:error (ex-message e)])))"))
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
fn vector_of_behaves_as_jvm_clojure() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        session.eval("(def v (vector-of :long 1 2 3))").unwrap();
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
            ("(conj (vector-of :byte) 200)", r#"[:error "Value out of range for byte: 200"]"#),
            ("(reduce + (into (vector-of :long) (range 1000)))", "499500"),
            ("(v 2)", "3"),
            ("(seq (vector-of :short 7 8))", "(7 8)"),
            ("(vector-of :foo)", r#"[:error "Unrecognized type :foo"]"#),
            ("(get v 5 :nf)", ":nf"),
            ("(pop (vector-of :long))", r#"[:error "Can't pop empty vector"]"#),
            // Leaves across a chunk boundary, assoc into a full leaf, and pop
            // back through it.
            ("(let [w (into (vector-of :long) (range 70))] [(nth w 31) (nth w 32) (nth (assoc w 5 -5) 5) (count (nth (iterate pop w) 40))])", "[31 32 -5 30]"),
        ] {
            assert_eq!(printed(&mut session, source), expected, "{source}");
        }
    }
}
