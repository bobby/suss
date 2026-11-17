//! reduce over arrays, strings, vectors, subvecs and vector-of, and the
//! first-class arithmetic functions it calls, answer as Clojure does (#444):
//! early termination by reduced, inside a leaf and at its boundary, initial
//! values, empty collections, and arithmetic past i31's range and on overflow.
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
fn reductions_answer_as_clojure_does() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session
            .eval(
                "(def v (vec (range 100)))
                 (def vl (into (vector-of :long) (range 100)))
                 (def upto (fn [n] (fn [a x] (if (= x n) (reduced a) (+ a x)))))",
            )
            .unwrap();
        for (source, expected) in [
            // Vectors: whole, with an initial value, empty, stopped early.
            ("[(reduce + v) (reduce + 10 v) (reduce + []) (reduce + 5 [])]", "[4950 4960 0 5]"),
            ("[(reduce (upto 41) v) (reduce (upto 32) v) (reduce (upto 31) v) (reduce (upto 0) 7 v)]", "[820 496 465 7]"),
            ("(reduce conj [] (subvec v 30 70))", "[30 31 32 33 34 35 36 37 38 39 40 41 42 43 44 45 46 47 48 49 50 51 52 53 54 55 56 57 58 59 60 61 62 63 64 65 66 67 68 69]"),
            ("[(reduce + (subvec v 33 35)) (reduce + (subvec v 31 33)) (reduce + 1 (subvec v 5 5))]", "[67 63 1]"),
            ("(reduce (upto 40) (subvec v 30 70))", "345"),
            // vector-of: leaves and tail, stopped inside a leaf and at one.
            ("[(reduce + vl) (reduce + 1 vl) (reduce (upto 64) vl) (reduce (upto 70) vl)]", "[4950 4951 2016 2415]"),
            ("[(reduce + (into (vector-of :double) [1.5 2.5])) (reduce + (vector-of :long))]", "[4.0 0]"),
            // Arrays and strings.
            ("[(reduce + (long-array [1 2 3])) (reduce + (object-array [1 2 3])) (reduce + 1 (long-array 0)) (reduce + (long-array 0))]", "[6 6 1 0]"),
            ("[(reduce str \"abc\") (reduce str \"x\" \"bc\") (reduce + (double-array [0.5 0.25]))]", "[\"abc\" \"xbc\" 0.75]"),
            ("(reduce (fn [a x] (if (= x 3) (reduced :stop) a)) 0 (long-array [1 2 3 4]))", ":stop"),
            ("(reduce (fn [a x] (reduced [a x])) :init (object-array [:a :b]))", "[:init :a]"),
            ("[(reduce + 2 (long-array [1 2 3])) (reduce + 1 (object-array 0)) (reduce * (long-array 0)) (reduce str \"\")]", "[8 1 1 \"\"]"),
            // First-class arithmetic: each count, i31 bounds and overflow.
            ("(let [p (identity +) m (identity -) t (identity *)] [(p) (p 4) (p 1 2) (p 1 2 3) (m 5) (m 5 3) (m 9 3 2) (t 3 4) (t)])", "[0 4 3 6 -5 2 4 12 1]"),
            ("(let [p (identity +) m (identity -)] [(p 1073741823 1) (m -1073741824 1) (p -1073741824 -1) (p 1 2.5) (m 1.5 1)])", "[1073741824 -1073741825 -1073741825 3.5 0.5]"),
            ("(let [p (identity +)] (try (p 9223372036854775807 1) (catch :default e :threw)))", ":threw"),
            ("(let [p (identity +)] (try (p 1 :a) (catch :default e :threw)))", ":threw"),
            ("[(apply + [1 2]) (map - [1 2]) (reduce - 10 [1 2])]", "[3 (-1 -2) 7]"),
            // reduced? and instance? on numbers, nil and objects.
            ("(mapv (fn [x] [(reduced? x) (instance? Reduced x)]) [1 (reduced 2) nil :k [3]])", "[[false false] [true true] [false false] [false false] [false false]]"),
        ] {
            assert_eq!(printed(&mut session, source), expected, "{source}");
        }
    }
}
