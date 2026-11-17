//! A construction site builds a type's object inline once it holds the type
//! (#443), and `aclone` copies inline. Objects, errors, redefined
//! types and the collections built on them answer as before.
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
fn construction_sites_answer_as_before() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session
            .eval(
                "(deftype P [x y] Object (toString [_] (str x \",\" y)))
                 (defprotocol Bump (bump [m]))
                 (deftype M [^:mutable n] Bump (bump [m] (set! n (inc n)) m) Object (toString [_] (str n)))
                 (def make-p (fn [x y] (P. x y)))
                 (def ctor (fn [c x] (new c x)))",
            )
            .unwrap();
        for (source, expected) in [
            // The same site, warm and again, and distinct objects.
            ("(mapv (fn [i] (str (make-p i (* i i)))) (range 4))", "[\"0,0\" \"1,1\" \"2,4\" \"3,9\"]"),
            ("(let [a (make-p 1 2) b (make-p 1 2)] [(identical? a b) (.-x a) (.-y b) (instance? P a)])", "[false 1 2 true]"),
            ("(let [a (M. 1) b (M. 1)] (bump a) (bump a) [(str a) (str b)])", "[\"3\" \"1\"]"),
            // A constructor passed as a value; one site, two types.
            ("(mapv (fn [c] (str (ctor c 5))) [M M])", "[\"5\" \"5\"]"),
            // Errors take their own plans.
            ("[(ex-message (ex-info \"boom\" {:a 1})) (ex-data (ex-info \"b\" {:a 1})) (try (throw (ex-info \"t\" {})) (catch :default e (ex-message e)))]", "[\"boom\" {:a 1} \"t\"]"),
            // aclone copies, and copies are independent.
            ("(let [a (long-array [1 2 3]) b (aclone a)] (aset b 0 9) [(vec a) (vec b) (vec (aclone (object-array [:x nil])))])", "[[1 2 3] [9 2 3] [:x nil]]"),
            ("(try (aclone [1 2]) (catch :default e :threw))", ":threw"),
            // Collections built on deftypes: conj across tails and levels.
            ("(let [v (reduce conj [] (range 1100))] [(count v) (nth v 31) (nth v 32) (nth v 1056) (peek v) (= v (vec (range 1100)))])", "[1100 31 32 1056 1099 true]"),
            ("(let [v (vec (range 40)) w (conj v :a)] [(count v) (count w) (peek w) (nth w 39)])", "[40 41 :a 39]"),
            ("(let [m (reduce (fn [m i] (assoc m i (* 2 i))) {} (range 600))] [(count m) (get m 599) (get m 7)])", "[600 1198 14]"),
            ("(let [vl (into (vector-of :long) (range 70))] [(count (conj vl 1)) (peek (conj vl 99)) (nth (conj vl 5) 69)])", "[71 99 69]"),
        ] {
            assert_eq!(printed(&mut session, source), expected, "{source}");
        }
        // A redefined type: a site compiled before it builds the new type.
        session.eval("(deftype P [x y] Object (toString [_] (str \"new \" x)))").unwrap();
        assert_eq!(printed(&mut session, "(str (make-p 1 2))"), "\"new 1\"");
    }
}
