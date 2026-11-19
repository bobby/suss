//! User field reads and method calls through site caches (#462) answer as the
//! uncached lookup did: for one type or several at a site, after a type is
//! redefined, for built-in internals, private mutable fields and missing
//! names, and for owners that aren't source objects.
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
fn user_sites_answer_as_before() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session
            .eval(
                "(deftype P [a b] Object (sum [_] (+ a b)) (scale [_ k] (* k a)))
                 (deftype Q [b a] Object (sum [_] (- a b)) (scale [_ k] (+ k a)))
                 (deftype C [^:mutable n] Object (bump [this] (set! n (inc n)) (.-n this)) (peek-n [this] (.-n this)))
                 (def read-a (fn [x] (.-a x)))
                 (def call-sum (fn [x] (.sum x)))
                 (def call-scale (fn [x k] (.scale x k)))
                 (def outcome (fn [f] (try (f) (catch :default e :threw))))",
            )
            .unwrap();
        for (source, expected) in [
            // One type, warm, and two types with their fields in other slots.
            ("(mapv read-a [(P. 1 2) (P. 3 4) (Q. 5 6) (P. 7 8) (Q. 9 10)])", "[1 3 6 7 10]"),
            ("(mapv call-sum [(P. 1 2) (Q. 1 5) (P. 3 4) (Q. 2 9)])", "[3 4 7 7]"),
            ("(mapv (fn [x] (call-scale x 10)) [(P. 2 0) (Q. 0 3) (P. 4 0)])", "[20 13 40]"),
            // A missing field or method, after the site is warm.
            ("[(read-a (P. 1 2)) (outcome (fn [] (call-sum (C. 1)))) (call-sum (P. 1 1))]", "[1 :threw 2]"),
            // Mutable fields: private outside their type, readable inside.
            ("(let [c (C. 1)] [(.bump c) (.bump c) (.peek-n c) (outcome (fn [] (.-n c)))])", "[2 3 3 :threw]"),
            // Built-in internals stay sealed at a site warmed by a user type.
            ("(let [cnt (fn [x] (.-cnt x))] [(outcome (fn [] (cnt (P. 1 2)))) (outcome (fn [] (cnt [1 2 3])))])", "[nil :threw]"),
            ("(let [tail (fn [x] (.-tail x))] [(outcome (fn [] (tail [1 2]))) (outcome (fn [] (tail (P. 1 2)))) (outcome (fn [] (tail [3])))])", "[:threw nil :threw]"),
            ("(do (deftype T [tail] Object) (let [tail (fn [x] (.-tail x))] [(tail (T. 1)) (outcome (fn [] (tail [3]))) (tail (T. 2))]))", "[1 :threw 2]"),
            // Owners that aren't source objects share a site with ones that are.
            ("(let [len (fn [x] (.-length x))] [(len \"abc\") (outcome (fn [] (len (P. 1 2)))) (len \"de\")])", "[3 nil 2]"),
            ("(let [at (fn [x] (.charAt x 1))] [(at \"abc\") (outcome (fn [] (at (P. 1 2)))) (at \"xy\")])", "[\"b\" :threw \"y\"]"),
            ("(do (deftype S [] Object (charAt [_ i] (* 10 i))) (let [at (fn [x] (.charAt x 1))] [(at (S.)) (at \"abc\") (at (S.)) (at \"xy\")]))", "[10 \"b\" 10 \"y\"]"),
        ] {
            assert_eq!(printed(&mut session, source), expected, "{source}");
        }
        // A redefined type: sites compiled before it read the new layout.
        session
            .eval("(deftype P [z a b] Object (sum [_] (* a b)) (scale [_ k] k))")
            .unwrap();
        assert_eq!(printed(&mut session, "[(read-a (P. 0 5 6)) (call-sum (P. 0 5 6)) (call-scale (P. 0 1 1) 9) (read-a (Q. 1 2))]"), "[5 30 9 2]");
    }
}
