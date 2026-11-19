//! Maps and sets called as functions read as `get` and `contains?` do
//! (#467): present and missing keys, not-found values, keys the direct
//! lookup doesn't answer, and a set's own element.
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
fn maps_and_sets_called_as_functions() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session
            .eval(
                "(def km {:a 1 :b nil \"s\" 2 [1 2] 3 nil 4})
                 (def hm (zipmap (range 100) (map #(* 10 %) (range 100))))
                 (def hk (assoc hm :k 5 \"t\" 6 [1] 7))
                 (def st #{1 :a \"s\" [1 2] nil 1000000000000})
                 (def call1 (fn [f k] (f k)))
                 (def call2 (fn [f k d] (f k d)))",
            )
            .unwrap();
        for (source, expected) in [
            ("(mapv (fn [k] (call1 km k)) [:a :b :z \"s\" [1 2] nil 7])", "[1 nil nil 2 3 4 nil]"),
            ("(mapv (fn [k] (call2 km k :nf)) [:a :b :z \"s\" [1 2] nil 7])", "[1 nil :nf 2 3 4 :nf]"),
            ("(mapv (fn [k] (call1 hm k)) [0 99 100 -1 :a])", "[0 990 nil nil nil]"),
            ("(mapv (fn [k] (call2 hk k :nf)) [5 :k \"t\" [1] 100 :z])", "[50 5 6 7 :nf :nf]"),
            ("(mapv (fn [k] (call1 st k)) [1 :a \"s\" [1 2] nil 2 :b 1000000000000])", "[1 :a \"s\" [1 2] nil nil nil 1000000000000]"),
            ("(mapv (fn [k] (call2 st k :nf)) [1 2 nil :a :z])", "[1 :nf nil :a :nf]"),
            // A set answers its own element, which may be equal and not
            // identical to the key.
            ("(let [e [1 2] k (vec (list 1 2))] [(identical? (#{e} k) e) (identical? (#{e} k) k)])", "[true false]"),
            ("(let [s (set (range 50))] (mapv s [0 49 50 :a]))", "[0 49 nil nil]"),
            // The same calls through the var's own invoke paths.
            ("[(km :a) (km :z 0) (hm 3) (st 1) (st 3 :x) ({} :a) (#{} :a :x)]", "[1 0 30 1 :x nil :x]"),
            ("(try ({:a 1}) (catch :default e :threw))", ":threw"),
            // Other callables at the same call sites miss the direct lookup.
            ("[(call1 [10 20] 1) (try (call1 [10 20] 5) (catch :default e :threw)) (call1 :a {:a 9}) (call2 :b {:a 9} :nf)]", "[20 :threw 9 :nf]"),
            ("(do (deftype Fn1 [] IFn (-invoke [_ k] [:called k])) [(call1 (Fn1.) :a) (call1 km :a) (call1 (Fn1.) 1)])", "[[:called :a] 1 [:called 1]]"),
        ] {
            assert_eq!(printed(&mut session, source), expected, "{source}");
        }
    }
}
