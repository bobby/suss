//! Protocol calls on native receivers, multimethods and apply take cached or
//! direct paths (#445). Each still answers as Clojure does after the protocol
//! is extended, a method is added or preferred, or a var is redefined.
use std::io::Write;
use std::process::{Command, Stdio};
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

fn check(session: &mut Session, cases: &[(&str, &str)]) {
    for (source, expected) in cases {
        assert_eq!(printed(session, source), *expected, "{source}");
    }
}

#[test]
fn native_protocol_sites_follow_extension() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session
            .eval(
                "(defprotocol Shape (sides [x]) (scaled [x k]))
                 (deftype Tri [] Shape (sides [_] 3) (scaled [_ k] (* 3 k)))
                 (extend-type number Shape (sides [x] :number) (scaled [x k] (* x k)))
                 (def sides-of (fn [x] (sides x)))
                 (def scaled-of (fn [x k] (scaled x k)))
                 (def all-sides (fn [xs] (mapv sides-of xs)))",
            )
            .unwrap();
        check(
            &mut session,
            &[
                // The site's first kind, then another, then an object.
                ("[(sides-of 1) (sides-of 2.5) (scaled-of 4 5) (sides-of (->Tri)) (scaled-of (->Tri) 2)]", "[:number :number 20 3 6]"),
                // No implementation for a string yet: the protocol's error.
                ("(try (sides-of \"s\") (catch :default e :threw))", ":threw"),
            ],
        );
        // Extending a kind after the site has cached another.
        session.eval("(extend-type string Shape (sides [x] (count x)))").unwrap();
        check(&mut session, &[("(all-sides [1 \"abc\" (->Tri) 2 \"de\"])", "[:number 3 3 :number 2]")]);
        // Replacing a kind's implementation the site has cached.
        session.eval("(extend-type number Shape (sides [x] (inc x)))").unwrap();
        check(&mut session, &[("(all-sides [1 41 \"abc\"])", "[2 42 3]")]);
        // The protocol fn as a value: the dispatcher's own cache.
        check(&mut session, &[("(mapv sides [1 \"abc\" (->Tri) 2.5])", "[2 3 3 3.5]")]);
        // A default, and nil, which is its own kind.
        check(&mut session, &[("(try (sides-of :k) (catch :default e :threw))", ":threw")]);
        session.eval("(extend-type default Shape (sides [x] :default))").unwrap();
        check(&mut session, &[("(all-sides [:k nil [1] 7])", "[:default :default :default 8]")]);
        session.eval("(extend-type nil Shape (sides [x] :nil))").unwrap();
        check(&mut session, &[("(all-sides [:k nil 7])", "[:default :nil 8]")]);
        // A redefined protocol fn is called for every receiver.
        check(
            &mut session,
            &[
                ("(with-redefs [sides (fn [x] :redefined)] (all-sides [1 \"a\" (->Tri)]))", "[:redefined :redefined :redefined]"),
                ("(all-sides [1 \"a\" (->Tri)])", "[2 1 3]"),
                // A wrong operand count still reaches the arity error.
                ("(try ((fn [x] (scaled x)) 1) (catch :default e :threw))", ":threw"),
            ],
        );
    }
}

/// Each line's REPL display, in order: a definition shows its var.
fn repl(lines: &[(&str, &str)]) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_suss"))
        .arg("repl")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let input = lines.iter().map(|(source, _)| format!("{source}\n")).collect::<String>();
    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let shown = stdout.lines().collect::<Vec<_>>();
    assert_eq!(shown.len(), lines.len(), "{stdout}\n{}", String::from_utf8_lossy(&output.stderr));
    for ((source, expected), line) in lines.iter().zip(shown) {
        assert_eq!(line, *expected, "{source}");
    }
}

#[test]
fn multimethods_follow_methods_preferences_and_hierarchies() {
    repl(&[
        ("(defmulti area :shape)", "#'user/area"),
        ("(do (defmethod area :square [x] [:square (:side x)]) nil)", "nil"),
        ("(do (defmethod area :default [x] :default) nil)", "nil"),
        ("(defn area-of [x] (area x))", "#'user/area-of"),
        ("[(area-of {:shape :square :side 2}) (area-of {:shape :square :side 3}) (area-of {:shape :circle})]", "[[:square 2] [:square 3] :default]"),
        // A method added, replaced or removed after the site has called one.
        ("(do (defmethod area ::circle [x] :circle) [(area-of {:shape ::circle}) (area-of {:shape :square :side 1})])", "[:circle [:square 1]]"),
        ("(do (defmethod area :square [x] :square2) (area-of {:shape :square}))", ":square2"),
        ("(do (remove-method area :square) [(area-of {:shape :square}) (area-of {:shape ::circle})])", "[:default :circle]"),
        // The global hierarchy changing after the last call, an ambiguity, and
        // a preference added after a call.
        ("(area-of {:shape ::rect})", ":default"),
        ("(do (derive ::rect ::circle) (area-of {:shape ::rect}))", ":circle"),
        ("(do (defmethod area ::shape [x] :shape) (derive ::rect ::shape) (try (area-of {:shape ::rect}) (catch :default e :ambiguous)))", ":ambiguous"),
        ("(do (prefer-method area ::shape ::circle) [(area-of {:shape ::rect}) (area-of {:shape ::rect})])", "[:shape :shape]"),
        // A multimethod's own hierarchy, in an atom.
        ("(def h (atom (make-hierarchy)))", "#'user/h"),
        ("(defmulti kind identity :hierarchy h)", "#'user/kind"),
        ("(do (defmethod kind ::animal [x] :animal) (defmethod kind :default [x] :none) (kind ::dog))", ":none"),
        ("(do (swap! h derive ::dog ::animal) [(kind ::dog) (kind ::cat)])", "[:animal :none]"),
        // Other dispatch fns and arities, equal but not identical dispatch
        // values, and no method.
        ("(defmulti pair (fn [a b] [(keyword? a) (keyword? b)]))", "#'user/pair"),
        ("(do (defmethod pair [true true] [a b] :kk) [(pair :a :b) (pair :x :y) (try (pair 1 :b) (catch :default e :threw))])", "[:kk :kk :threw]"),
        ("(defmulti n-ary (fn [& xs] (count xs)))", "#'user/n-ary"),
        ("(do (defmethod n-ary 3 [a b c] [a b c]) (defmethod n-ary 0 [] :none) [(n-ary) (n-ary 1 2 3) (n-ary)])", "[:none [1 2 3] :none]"),
        ("(defmulti by-default :k :default ::none)", "#'user/by-default"),
        ("(do (defmethod by-default ::none [x] :fallback) [(by-default {:k 1}) (by-default {})])", "[:fallback :fallback]"),
    ]);
}

#[test]
fn callable_objects_and_apply_answer_as_calls() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session
            .eval(
                "(defprotocol Named (label [x]))
                 (deftype Adder [n] IFn (-invoke [_ a] (+ n a)) (-invoke [_ a b] (+ n a b)))
                 (deftype Plain [])
                 (def call0 (fn [f] (f)))
                 (def call1 (fn [f x] (f x)))
                 (def call2 (fn [f x y] (f x y)))
                 (def kw :a)
                 (def variadic (fn [& xs] (vec xs)))
                 (def two (fn [a b] [a b]))",
            )
            .unwrap();
        check(
            &mut session,
            &[
                // One site, several callable kinds in turn, more than its
                // cache holds.
                ("(mapv (fn [f] (call1 f 2)) [(->Adder 1) {2 :two} #{2} [:a :b :c] (->Adder 10) (fn [x] [x])])", "[3 :two 2 :c 12 [2]]"),
                ("(mapv (fn [f] (call1 f {:a 1})) [kw :a :b])", "[1 1 nil]"),
                ("[(call2 (->Adder 1) 2 3) (call2 kw {} :nf) (call2 {:x 1} :y :nf) (call2 #{1} 2 :nf)]", "[6 :nf :nf :nf]"),
                // Arities a type lacks, and values that aren't callable.
                ("(try (call0 (->Adder 1)) (catch :default e :threw))", ":threw"),
                ("(try (call1 (->Plain) 1) (catch :default e :threw))", ":threw"),
                ("(try (call1 nil 1) (catch :default e :threw))", ":threw"),
                ("(try (call1 5 1) (catch :default e :threw))", ":threw"),
                // apply over a short vector, a long one, lists and seqs.
                ("[(apply two [1 2]) (apply two 1 [2]) (apply + 1 2 [3 4]) (apply + 1 2 3 [4 5 6 7])]", "[[1 2] [1 2] 10 28]"),
                ("[(apply variadic []) (apply variadic 1 [2 3]) (apply variadic (range 6)) (apply str (vec (range 12)))]", "[[] [1 2 3] [0 1 2 3 4 5] \"01234567891011\"]"),
                ("[(apply max 1 (list 5 2)) (apply (->Adder 1) [2]) (apply kw [{:a 7}]) (apply {:k 1} [:k])]", "[5 3 7 1]"),
                ("[(take 3 (apply concat (repeat [1]))) (apply (fn [a & r] (take 2 r)) 0 (range))]", "[(1 1 1) (0 1)]"),
                ("(try (apply two [1]) (catch :default e :threw))", ":threw"),
                ("(try (apply nil [1]) (catch :default e :threw))", ":threw"),
            ],
        );
        // Extending a type with IFn after a site has called another.
        session.eval("(extend-type Plain IFn (-invoke [_ a] [:plain a]))").unwrap();
        check(&mut session, &[("[(call1 (->Plain) 1) (call1 (->Adder 1) 1)]", "[[:plain 1] 2]")]);
    }
}
