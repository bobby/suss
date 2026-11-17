//! assoc replaces a hash map's value through the runtime, and anything else
//! through core (#443); multiplication of two small longs, bit-count and
//! vector-of's conj take shorter paths too. All answer as Clojure does.
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
fn updates_answer_as_clojure_does() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        // A three-level trie takes more than 32,800 entries to build.
        session.set_operation_fuel(2_000_000_000);
        session
            .eval(
                "(def m (zipmap (range 1024) (range 1024)))
                 (def big (zipmap (range 40000) (range 40000)))
                 (def km (zipmap (map (fn [i] (keyword (str \"k\" i))) (range 100)) (range 100)))
                 (def small {:a 1 :b 2})
                 (def bump (fn [m k] (assoc m k (inc (get m k 0)))))",
            )
            .unwrap();
        for (source, expected) in [
            // Replacing values, at every level, and the map's other parts.
            ("(let [n (assoc m 7 :x)] [(get n 7) (get m 7) (count n) (= (dissoc n 7) (dissoc m 7))])", "[:x 7 1024 true]"),
            ("(let [n (reduce (fn [a i] (assoc a i (- i))) m (range 1024))] [(count n) (get n 1023) (= n (zipmap (range 1024) (map - (range 1024))))])", "[1024 -1023 true]"),
            ("(let [n (assoc big 39999 :end 33000 :mid)] [(get n 39999) (get n 33000) (get n 5) (count n)])", "[:end :mid 5 40000]"),
            ("(let [n (assoc km :k5 :five)] [(get n :k5) (get km :k5) (count n)])", "[:five 5 100]"),
            // The same value: the same map, as core answers.
            ("[(identical? (assoc m 3 3) m) (identical? (assoc km :k1 1) km)]", "[true true]"),
            // Equality, hashing and metadata of an updated map.
            ("(let [n (assoc m 1 :one)] [(= n (assoc (into {} m) 1 :one)) (= (hash n) (hash (assoc (into {} m) 1 :one)))])", "[true true]"),
            ("(meta (assoc (with-meta m {:tag 1}) 2 :two))", "{:tag 1}"),
            // New keys, other keys and other collections: core's paths.
            ("(let [n (assoc m 5000 :new)] [(count n) (get n 5000) (count m)])", "[1025 :new 1024]"),
            ("(let [n (assoc m nil :nil)] [(count n) (get n nil) (contains? m nil)])", "[1025 :nil false]"),
            ("(let [n (assoc m \"s\" 1 1.5 2)] [(count n) (get n \"s\") (get n 1.5)])", "[1026 1 2]"),
            ("[(assoc small :a 9) (assoc small :c 3) (assoc nil :a 1) (assoc [1 2] 1 :x) (assoc [1 2] 2 :y)]", "[{:a 9, :b 2} {:a 1, :b 2, :c 3} {:a 1} [1 :x] [1 2 :y]]"),
            ("(try (assoc [1 2] 5 :z) (catch :default e :threw))", ":threw"),
            ("(reduce (fn [a w] (bump a w)) {} [:a :b :a :c :a :b])", "{:a 3, :b 2, :c 1}"),
            // Multiplication of i31 longs, to and past the i31 range.
            ("[(* 3 4) (* -1073741824 2) (* 1073741823 1073741823) (* -1073741824 -1073741824) (* 0 -5)]", "[12 -2147483648 1152921502459363329 1152921504606846976 0]"),
            ("[(* 2 1.5) (* 3 (long 1e15)) (try (* 2 nil) (catch :default e :threw))]", "[3.0 3000000000000000 :threw]"),
            ("(try (* 9223372036854775807 2) (catch :default e :threw))", ":threw"),
            // bit-count is a popcount of the int32.
            ("(mapv bit-count [0 1 -1 255 0x55555555 0xFFFFFFFF 0x100000000 -12345 2.5])", "[0 1 32 8 16 32 0 27 1]"),
            // vector-of conj across its leaves.
            ("(let [vl (into (vector-of :long) (range 1024)) w (conj vl 7)] [(count w) (peek w) (nth w 1023) (nth w 31) (count vl)])", "[1025 7 1023 31 1024]"),
            ("(= (reduce conj (vector-of :long) (range 2000)) (range 2000))", "true"),
            ("[(into (vector-of :double) [1 2.5]) (count (reduce conj (vector-of :int) (range 70)))]", "[[1.0 2.5] 70]"),
        ] {
            assert_eq!(printed(&mut session, source), expected, "{source}");
        }
    }
}
