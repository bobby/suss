//! The paths bench.py's programs take (#446) answer as before: `instance?`
//! sites that cache a negative answer, core field writes through a site,
//! unhinted object-array reads and writes inline, and reduce over a chunked
//! lazy seq a chunk at a time.
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
fn instance_sites_answer_as_before() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session
            .eval(
                "(deftype A [])
                 (deftype B [])
                 (deftype C [])
                 (def b? (fn [x] (instance? B x)))",
            )
            .unwrap();
        for (source, expected) in [
            // One site: a negative answer, a positive one, then each again,
            // and values that aren't source objects.
            ("(mapv b? [(A.) (B.) (A.) (C.) (B.) (A.) nil 1 :k \"s\" [] (ex-info \"e\" {})])",
             "[false true false false true false false false false false false false]"),
            // ExceptionInfo, whose prototype has a parent, at a site that sees
            // other types before and after (#457).
            ("(let [info? (fn [x] (instance? ExceptionInfo x))] (mapv info? [(A.) (ex-info \"a\" {}) (B.) (ex-info \"b\" {}) (A.)]))",
             "[false true false true false]"),
            // reduced? compiles inline (#457): its answers, a local or user
            // var of that name, and a wrong arity, which still throws.
            ("[(reduced? (reduced 1)) (reduced? 1) (reduced? nil) (reduced? {:a 1}) (reduced? (A.)) (reduced? @(reduced (reduced 2)))]",
             "[true false false false false true]"),
            ("(let [reduced? (fn [x] :local)] (reduced? 1))", ":local"),
            ("(do (def ^:private reduced? (fn [x] :mine)) (reduced? 1))", ":mine"),
            ("(try (apply reduced? [1 2]) (catch :default e :threw))", ":threw"),
            // reduced? on accumulators of several types.
            ("(reduce (fn [a x] (if (= x 5) (reduced a) (conj a x))) [] (range 10))", "[0 1 2 3 4]"),
            ("(persistent! (reduce (fn [a x] (if (= x 3) (reduced a) (conj! a x))) (transient []) (range 10)))", "[0 1 2]"),
            ("(reduce (fn [a x] (if (= x 4) (reduced a) (assoc a x x))) {} (range 10))", "{0 0, 1 1, 2 2, 3 3}"),
        ] {
            assert_eq!(printed(&mut session, source), expected, "{source}");
        }
        // A prototype change reaches a site that cached A as not a B.
        let fresh = |session: &mut Session| printed(session, "[(b? (A.)) (instance? B (A.))]");
        assert_eq!(fresh(&mut session), "[false false]");
        session.eval("(set! (.-__proto__ (.-prototype A)) (.-prototype B))").unwrap();
        let after = fresh(&mut session);
        let (cached, uncached) = after.trim_matches(['[', ']']).split_once(' ').unwrap();
        assert_eq!(cached, uncached, "a cached site answers as a new one: {after}");
        // A redefined type: the site's constructor changes.
        session.eval("(deftype B [])").unwrap();
        assert_eq!(printed(&mut session, "[(b? (B.)) (b? (C.))]"), "[true false]");
    }
}

#[test]
fn core_field_writes_answer_as_before() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        for (source, expected) in [
            // Transient maps write their Box's val and nodes' fields.
            ("(let [m (persistent! (reduce (fn [m x] (assoc! m x (* x x))) (transient {}) (range 1000)))] [(count m) (get m 999) (get m 0) (get m 1000)])",
             "[1000 998001 0 nil]"),
            ("(let [m (into {} (map (fn [x] [x x])) (range 100))] [(count m) (get m 99) (contains? m 100)])", "[100 99 false]"),
            ("(let [m (persistent! (reduce (fn [m x] (dissoc! m x)) (transient (zipmap (range 40) (range 40))) (range 0 40 2)))] [(count m) (get m 1) (get m 2)])",
             "[20 1 nil]"),
            // Transient vectors, across tails and levels, and sets.
            ("(let [v (persistent! (reduce conj! (transient []) (range 1100)))] [(count v) (nth v 31) (nth v 1056) (peek v)])", "[1100 31 1056 1099]"),
            ("(let [v (persistent! (assoc! (transient (vec (range 40))) 39 :x 0 :y))] [(count v) (peek v) (first v)])", "[40 :x :y]"),
            ("(count (persistent! (reduce conj! (transient #{}) (range 300))))", "300"),
            // A lazy seq caches its realized value: each element realized once.
            ("(let [n (atom 0) s (map (fn [x] (swap! n inc) x) [1 2])] [(first s) (first s) (count s) (vec s) @n])", "[1 1 2 [1 2] 2]"),
            // Mutable fields of a user type, beside core's.
            ("(do (deftype Counter [^:mutable n] Object (bump [_] (set! n (inc n)) n)) (let [c (Counter. 0)] (.bump c) [(.bump c) (.bump (Counter. 7))]))", "[2 8]"),
        ] {
            assert_eq!(printed(&mut session, source), expected, "{source}");
        }
        // Writing a transient after persistent! still throws.
        assert_eq!(
            printed(&mut session, "(let [t (transient [])] (persistent! t) (try (conj! t 1) (catch :default e :threw)))"),
            ":threw"
        );
    }
}

#[test]
fn unhinted_array_access_answers_as_before() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session
            .eval("(def get-at (fn [a i] (aget a i))) (def set-at (fn [a i x] (aset a i x)))")
            .unwrap();
        for (source, expected) in [
            ("(let [a (make-array 3)] [(set-at a 1 :x) (get-at a 1) (get-at a 0) (vec a)])", "[:x :x nil [nil :x nil]]"),
            ("(let [a (object-array [1 2 3])] (set-at a 2 nil) [(get-at a 0) (get-at a 2) (alength a)])", "[1 nil 3]"),
            // Other kinds and shapes take the generic path, as before.
            ("(let [a (long-array [1 2 3])] (set-at a 0 9) [(get-at a 0) (get-at a 2)])", "[9 3]"),
            ("(let [a (double-array 2)] (set-at a 1 1.5) [(get-at a 0) (get-at a 1)])", "[0.0 1.5]"),
            ("(let [a (into-array [:a :b])] [(get-at a 1) (get-at a 0)])", "[:b :a]"),
            // Out of range, or a non-integer index, throws as before.
            ("(try (get-at (make-array 2) 2) (catch :default e :threw))", ":threw"),
            ("(try (get-at (make-array 2) -1) (catch :default e :threw))", ":threw"),
            ("(try (set-at (make-array 2) 5 :x) (catch :default e :threw))", ":threw"),
            ("(try (get-at (make-array 2) nil) (catch :default e :threw))", ":threw"),
        ] {
            assert_eq!(printed(&mut session, source), expected, "{source}");
        }
    }
}

#[test]
fn chunked_reductions_answer_as_before() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        for (source, expected) in [
            ("(reduce + (map inc (filter even? (vec (range 100)))))", "2500"),
            ("(reduce + 0 (map inc (vec (range 100))))", "5050"),
            ("(reduce + (map inc (vec (range 100))))", "5050"),
            ("(reduce + 0 (map inc [1 2 3]))", "9"),
            ("(reduce + 0 (map inc []))", "0"),
            // A filter's chunk is shorter than its array.
            ("(= (reduce conj [] (filter odd? (vec (range 70)))) (vec (filter odd? (range 70))))", "true"),
            // reduced inside a chunk, and at a chunk's last element.
            ("(reduce (fn [a x] (if (> x 40) (reduced a) (+ a x))) 0 (map identity (vec (range 100))))", "820"),
            ("(reduce (fn [a x] (if (= x 31) (reduced [a x]) (+ a x))) 0 (map identity (vec (range 100))))", "[465 31]"),
            // Elements in order, each once.
            ("(let [seen (atom [])] (reduce (fn [_ x] (swap! seen conj x)) nil (map identity (vec (range 70)))) (= @seen (vec (range 70))))", "true"),
            ("(let [n (atom 0)] (reduce + 0 (map (fn [x] (swap! n inc) x) (vec (range 70)))) @n)", "70"),
            // Chunked, then not.
            ("(reduce + 0 (concat (map inc (vec (range 40))) (list 1 2 3)))", "826"),
            ("(reduce + 0 (cons 1 (map inc (vec (range 3)))))", "7"),
            ("(reduce + 0 (rest (map inc (vec (range 40)))))", "819"),
            ("(transduce (comp (map inc) (filter even?)) + 0 (vec (range 100)))", "2550"),
            ("(apply str (map str (vec (range 12))))", "\"01234567891011\""),
        ] {
            assert_eq!(printed(&mut session, source), expected, "{source}");
        }
    }
}
