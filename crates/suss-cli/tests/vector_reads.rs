//! `nth`, `first` and `get` read a PersistentVector's element through the
//! runtime, and anything else through core (#433). Both answer as Clojure
//! does: bounds, not-found, nil elements, and other collections.
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
fn vector_reads_answer_as_clojure_does() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        // A three-level trie takes more than 32,800 elements to build.
        session.set_operation_fuel(1_000_000_000);
        session
            .eval(
                "(def small [nil 1 2])
                 (def big (vec (range 100)))
                 (def deep (vec (range 1100)))
                 (def deeper (vec (range 40000)))
                 (def reads (fn [v i] [(nth v i :none) (get v i) (get v i :none)]))",
            )
            .unwrap();
        for (source, expected) in [
            // The tail, one trie level, two and three.
            ("(reads small 0)", "[nil nil nil]"),
            ("(reads big 99)", "[99 99 99]"),
            ("(reads big 50)", "[50 50 50]"),
            ("(reads deep 1050)", "[1050 1050 1050]"),
            ("(reads deep 5)", "[5 5 5]"),
            ("(reads deeper 33000)", "[33000 33000 33000]"),
            ("(reads deeper 39999)", "[39999 39999 39999]"),
            ("[(nth big 7) (nth deeper 1234) (first big) (first small)]", "[7 1234 0 nil]"),
            // Out of range, and keys that aren't indexes.
            ("(reads big 100)", "[:none nil :none]"),
            ("(reads big -1)", "[:none nil :none]"),
            ("(reads [] 0)", "[:none nil :none]"),
            ("[(get big :a) (get big :a :none) (get big nil)]", "[nil :none nil]"),
            ("(try (nth big 100) (catch :default e :threw))", ":threw"),
            ("(try (nth [] 0) (catch :default e :threw))", ":threw"),
            // Other vectors and collections take core's path.
            ("(reads (vector-of :long 1 2 3) 1)", "[2 2 2]"),
            ("(first (vector-of :long 4 5))", "4"),
            ("(reads (subvec big 10 20) 3)", "[13 13 13]"),
            ("(nth (list 1 2 3) 2)", "3"),
            ("(nth \"abc\" 1)", "\"b\""),
            ("[(first []) (first nil) (first (list 1 2)) (first \"ab\") (first {:a 1}) (first (range 3 5))]",
             "[nil nil 1 \"a\" [:a 1] 3]"),
            ("[(first (rest big)) (first (seq big))]", "[1 0]"),
        ] {
            assert_eq!(printed(&mut session, source), expected, "{source}");
        }
    }
}
