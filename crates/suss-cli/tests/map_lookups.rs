//! `get` and `contains?` read maps and sets through the runtime when the key
//! is an i31 long or an interned keyword, and through core otherwise (#442).
//! Both paths answer as Clojure does: a nil value is present, not-found is
//! returned only for an absent key, and other collections and keys take
//! core's general path.
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
fn lookups_answer_as_clojure_does() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session
            .eval(
                "(def big (zipmap (range 100) (range 100)))
                 (def nils (assoc big 7 nil))
                 (def negative (zipmap (range -50 50) (range 100)))
                 (def wide (assoc big 4294967296 :wide -2000000000 :low))
                 (def keyed (zipmap (map (fn [i] (keyword (str \"k\" i))) (range 40)) (range 40)))
                 (def small {:a 1 :b nil})
                 (def big-set (set (range 100)))
                 (def small-set #{1 2 3})
                 (def lookup (fn [m k] [(get m k) (get m k :none) (contains? m k)]))",
            )
            .unwrap();
        for (source, expected) in [
            // A hash map of longs: present, absent, a nil value.
            ("(lookup big 5)", "[5 5 true]"),
            ("(lookup big 500)", "[nil :none false]"),
            ("(lookup nils 7)", "[nil nil true]"),
            ("(lookup negative -7)", "[43 43 true]"),
            // Longs outside i31 range take core's path.
            ("(lookup wide 4294967296)", "[:wide :wide true]"),
            ("(lookup wide -2000000000)", "[:low :low true]"),
            // Keywords in a hash map and an array map.
            ("(lookup keyed :k39)", "[39 39 true]"),
            ("(lookup keyed :nope)", "[nil :none false]"),
            ("(lookup small :b)", "[nil nil true]"),
            ("(lookup small :c)", "[nil :none false]"),
            ("[(get small :a) (get small :b :none) (contains? small :a)]", "[1 nil true]"),
            // Sets: a hash set and a small one.
            ("(lookup big-set 99)", "[99 99 true]"),
            ("(lookup big-set 100)", "[nil :none false]"),
            ("(lookup small-set 2)", "[2 2 true]"),
            ("(lookup small-set 9)", "[nil :none false]"),
            // Other collections and keys.
            ("(lookup [10 20] 1)", "[20 20 true]"),
            ("(lookup [10 20] 2)", "[nil :none false]"),
            ("(lookup nil 1)", "[nil :none false]"),
            ("(lookup {\"a\" 1} \"a\")", "[1 1 true]"),
            ("(lookup \"abc\" 1)", "[\"b\" \"b\" true]"),
            ("[(get big 3) (get big 300 :none) (contains? big-set 4)]", "[3 :none true]"),
        ] {
            assert_eq!(printed(&mut session, source), expected, "{source}");
        }
    }
}
