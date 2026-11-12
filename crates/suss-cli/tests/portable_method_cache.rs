//! Core's method call sites cache each receiver type's Object method
//! (#378): two types per site, valid until any method table changes. The
//! results must be the full lookup's: across more types than a site keeps,
//! after a method is replaced, after a type is redefined, and in hash maps
//! whose tries mix ArrayNode, BitmapIndexedNode and HashCollisionNode.
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
fn cached_methods_match_the_full_lookup() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(2_000_000_000);
        for definition in [
            r#"(deftype A [] Object (toString [_] "a"))"#,
            r#"(deftype B [] Object (toString [_] "b"))"#,
            r#"(deftype C [] Object (toString [_] "c"))"#,
            // Keys whose hashes collide, so maps of them hold collision nodes.
            "(deftype K [n] IHash (-hash [_] (mod n 3)) IEquiv (-equiv [_ o] (and (instance? K o) (= n (.-n o)))))",
            "(def a (A.))",
        ] {
            session.eval(definition).unwrap_or_else(|error| panic!("{definition}: {error}"));
        }
        for (source, expected) in [
            // More types at one site (str's .toString) than it caches.
            ("(apply str (map str [(A.) (B.) (C.) (A.) (C.) (B.) (A.) (A.) (C.)]))", r#""abcacbaac""#),
            ("(apply str (map str (take 30 (cycle [(A.) (B.) (C.)]))))", r#""abcabcabcabcabcabcabcabcabcabc""#),
            // A method replaced after the site cached it, then the type redefined.
            ("(str a)", r#""a""#),
            (r#"(do (extend-type A Object (toString [_] "a2")) (str a))"#, r#""a2""#),
            ("(apply str (map str [a (B.) a (C.) a]))", r#""a2ba2ca2""#),
            (r#"(do (deftype A [] Object (toString [_] "a3")) [(str (A.)) (str a)])"#, r#"["a3" "a2"]"#),
            // Hash maps and sets: ArrayNode roots and children, bitmap nodes and
            // collision nodes, alternating at the same call sites.
            ("(let [m (zipmap (range 5000) (range 5000))] [(count m) (reduce + (map #(get m %) (range 5000))) (get m 5000 :none)])", "[5000 12497500 :none]"),
            ("(let [m (reduce #(assoc %1 %2 (* 2 %2)) {} (range 3000)) m (reduce dissoc m (range 0 3000 3))] [(count m) (get m 1) (get m 3 :gone) (contains? m 2999)])", "[2000 2 :gone true]"),
            ("(let [ks (map #(K. %) (range 300)) m (zipmap ks (range 300))] [(count m) (reduce + (map #(get m %) ks)) (get m (K. 300) :none) (get m (K. 7))])", "[300 44850 :none 7]"),
            ("(let [ks (map #(K. %) (range 300)) m (reduce dissoc (zipmap ks (range 300)) (take 150 ks))] [(count m) (get m (K. 0) :gone) (get m (K. 299))])", "[150 :gone 299]"),
            ("(let [m (merge (zipmap (range 100) (range 100)) (zipmap (map #(K. %) (range 100)) (range 100)))] [(count m) (get m 50) (get m (K. 50)) (get m (K. 100) :none)])", "[200 50 50 :none]"),
            ("(let [s (into #{} (concat (range 2000) (map #(K. %) (range 50))))] [(count s) (contains? s 1999) (contains? s (K. 49)) (contains? s (K. 50))])", "[2050 true true false]"),
            ("(let [t (persistent! (reduce #(assoc! %1 %2 %2) (transient {}) (range 2000)))] [(count t) (get t 1234)])", "[2000 1234]"),
        ] {
            assert_eq!(printed(&mut session, source), expected, "{source}");
        }
    }
}
