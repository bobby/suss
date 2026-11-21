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

#[test]
fn persistent_keyword_updates_keep_snapshots_metadata_and_values_after_gc() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval(r#"
          (def retained-map-readers
            (mapv (fn [n]
              (let [ks (vec (map (fn [i] (keyword (str "copy" i))) (range n)))
                    original (with-meta (zipmap ks (range n)) {:stamp :old})
                    changed (reduce (fn [m i]
                              (assoc m (nth ks i) [i 9007199254740993])) original (range n))
                    final (assoc changed (nth ks 0) nil (nth ks (dec n)) false :new :added)]
                (fn []
                  (let [expected (vec (map (fn [i] [i 9007199254740993]) (range n)))]
                    [(count original) (count changed) (count final) (meta final)
                     (= (vec (map (fn [k] (get original k :missing)) ks)) (vec (range n)))
                     (= (vec (map (fn [k] (get changed k :missing)) ks)) expected)
                     (= (vec (map (fn [k] (get final k :missing)) ks))
                        (assoc expected 0 nil (dec n) false))
                     (get final :new) (map? final)])))) [1 9 37 129]))"#).unwrap();
        session.collect().unwrap();
        // Full value comparisons happen before returning this bounded result.
        // The same public forms execute on JVM Clojure 1.12.6; no compiler
        // data or built-in internal identity is used as the behavior oracle.
        assert_eq!(printed(&mut session, "(mapv (fn [f] (f)) retained-map-readers)"),
          "[[1 1 2 {:stamp :old} true true true :added true] [9 9 10 {:stamp :old} true true true :added true] [37 37 38 {:stamp :old} true true true :added true] [129 129 130 {:stamp :old} true true true :added true]]");
        session.collect().unwrap();
        assert_eq!(printed(&mut session,
            "(let [f (nth retained-map-readers 2)] (= (f) (f)))"), "true");
    }
}
