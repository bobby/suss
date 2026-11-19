//! `^long` and `^double` function parameters are Clojure's checked longCast
//! and doubleCast of the argument, kept raw in the body (#385): arithmetic and
//! loops seeded from them stay unboxed beyond i31. Each case's AOT result must
//! equal the REPL's, and fixed expected values. Operands derive from the
//! argument `n`, 0.
use std::process::Command;
use suss_cli::portable_session::Session;

const DEFINITIONS: &str = "(def safe (fn [f] (try (f) (catch :default e :error))))
(def ident (fn [^long x] x))
(def dbl (fn [^double x] x))
(def count-from (fn [^long start ^long steps] (loop [i start k 0] (if (< k steps) (recur (inc i) (inc k)) i))))
(def sum-to (fn [^long n] (loop [i 0 acc 0] (if (< i n) (recur (inc i) (+ acc i)) acc))))
(def down (fn [^long n acc] (if (zero? n) acc (recur (dec n) (conj acc n)))))
(def arities (fn ([^long a] [:one (inc a)]) ([^long a ^double b] [:two (+ a b)])))
(def variadic (fn [^long a & more] [(inc a) more]))
(def mixed (fn [^long a {:keys [b]}] (+ a b)))
(def capture (fn [^long a] (fn [] (+ a 1))))
(def ret (fn ^long [^long a] (* a 2)))
(def ret-long (fn ^long [x] x))
(def ret-double (fn ^double [x] x))
(def ret-recur (fn again ^long [n x] (if (zero? n) x (recur (dec n) x))))
(def ret-multi (fn (^long [x] x) (^double [x y] (+ x y))))
(def checked-pair (fn ^long [^long x ^long y] (+ x y)))
(def hinted-redefined (fn ^long [^long x] (* x 2)))
(def before-redefinition (fn [x] (hinted-redefined x)))
(def hinted-redefined (fn [x] [:changed x]))
(def hinted-change-kind (fn ^long [^long x] (* x 2)))
(def before-kind-change (fn [x] (hinted-change-kind x)))
(def hinted-change-kind (fn ^double [x] (+ x 0.5)))
(def ^long ret-plain (fn [x] x))";
// Exercise the source defn macro in AOT; Session::eval intentionally does not
// expand macros, so its equivalent functions put the tags on their vectors.
const NAMED_DEFINITIONS: &str = "(defn ^long ret-named [x] x)
(defn ^double ret-named-multi ([x] x) ([x y] (+ x y)))
(defn ^long ret-nested [] (let [f (fn [] 1.75)] (if (= (f) 1.75) 2.75 nil)))
(defn ret-name-call ^double [x] (ret-named x))
(defn ^long ret-override ^double [x] x)";
const NAMED_EQUIVALENTS: &str = "(def ret-named (fn ^long [x] x))
(def ret-named-multi (fn (^double [x] x) (^double [x y] (+ x y))))
(def ret-nested (fn ^long [] (let [f (fn [] 1.75)] (if (= (f) 1.75) 2.75 nil))))
(def ret-name-call (fn ^double [x] (ret-named x)))
(def ret-override (fn ^double [x] x))";
const CASES: &[(&str, &str)] = &[
    ("casts", "[(ident (+ n 5)) (ident 3000000000) (ident 2.7) (ident -2.7) (ident 9223372036854775807) (dbl (+ n 3)) (dbl 1.5) (dbl 3000000000)]"),
    ("errors", "[(safe (fn [] (ident :k))) (safe (fn [] (ident nil))) (safe (fn [] (ident 1.0e30))) (safe (fn [] (dbl \"x\")))]"),
    ("loops", "[(count-from (+ n 3000000000) 5) (count-from -1073741826 3) (sum-to (+ n 100000)) (safe (fn [] (count-from 9223372036854775806 3)))]"),
    ("recur", "(down (+ n 4) [])"),
    ("arities", "[(arities (+ n 1)) (arities 2 0.5) (variadic (+ n 1) 2 3) (variadic 0) (mixed 2 {:b 3})]"),
    ("closures", "[((capture (+ n 4))) ((capture 3000000000)) (ret (+ n 21))]"),
    ("returns", "[(ret-long 2.75) (apply ret-long [2.75]) (ret-double (+ n 3)) (apply ret-double [3]) (ret-recur 10000 4.75) (ret-multi 5.75) (ret-multi 2 3)]"),
    ("return-special", "[(ret-double -0.0) (ret-double ##NaN) (ret-double ##Inf) (ret-long 1.0e30) (ret-long -1.0e30) (ret-long ##NaN) (safe (fn [] (ret-long nil))) (safe (fn [] (ret-double :k)))]"),
    ("cast-order", "(let [effects (atom [])] [(safe (fn [] (checked-pair (do (swap! effects conj :first) :bad) (do (swap! effects conj :second) 2)))) @effects])"),
    ("redefinition", "[(before-redefinition (+ n 3)) (before-kind-change (+ n 3))]"),
    ("return-names", "[(ret-named 1.75) (ret-named-multi 2) (ret-named-multi 2 3) (ret-nested) (ret-override 4) (apply ret-plain [1.75]) (ret-name-call 3.75)]"),
];

#[test]
fn primitive_parameter_hints_match_the_repl() {
    let root = tempfile::tempdir().unwrap();
    let mut app = format!("(ns app)\n{DEFINITIONS}\n{NAMED_DEFINITIONS}\n");
    for (name, source) in CASES {
        app.push_str(&format!("(defn case-{name} [n] (pr-str {source}))\n"));
    }
    std::fs::write(root.path().join("app.sus"), &app).unwrap();
    let exports = CASES.iter().map(|(name, _)| format!("export case-{name}: func(n: u32) -> string;")).collect::<Vec<_>>();
    std::fs::write(root.path().join("app.wit"), format!("package test:prim; world w {{ {} }}", exports.join(" "))).unwrap();
    let mut compile = Command::new(env!("CARGO_BIN_EXE_suss"));
    compile.current_dir(root.path()).args(["compile", "app.sus", "-w", "app.wit", "-o", "app.wasm"]);
    for (name, _) in CASES {
        compile.args(["--export", &format!("case-{name}=app/case-{name}")]);
    }
    let output = compile.output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));

    let mut repl = Session::new_repl().unwrap();
    repl.set_operation_fuel(200_000_000);
    repl.eval(DEFINITIONS).unwrap();
    repl.eval(NAMED_EQUIVALENTS).unwrap();
    for (name, source) in CASES {
        let output = Command::new(env!("CARGO_BIN_EXE_suss"))
            .current_dir(root.path())
            .args(["run", "app.wasm", "--invoke", &format!("case-{name}"), "--", "0"])
            .output()
            .unwrap();
        assert!(output.status.success(), "{source}: {}", String::from_utf8_lossy(&output.stderr));
        let aot = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let value = repl.eval(&format!("((fn [n] (pr-str {source})) 0)")).unwrap_or_else(|error| panic!("{source}: {error}"));
        let live = repl
            .inspect(&value, |mut store, value| {
                let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                Ok(array.elems(&mut store)?.map(|unit| unit.unwrap_i32() as u16).collect::<Vec<_>>())
            })
            .unwrap();
        let live = String::from_utf16(&live).unwrap();
        assert_eq!(aot, live, "{name}: {source}");
        // The expected values, independent of either implementation.
        let expected = match *name {
            "casts" => Some("[5 3000000000 2 -2 9223372036854775807 3.0 1.5 3.0E9]"),
            "errors" => Some("[:error :error :error :error]"),
            "loops" => Some("[3000000005 -1073741823 4999950000 :error]"),
            "recur" => Some("[4 3 2 1]"),
            "arities" => Some("[[:one 2] [:two 2.5] [2 (2 3)] [1 nil] 5]"),
            "closures" => Some("[5 3000000001 42]"),
            "returns" => Some("[2 2 3.0 3.0 4 5 5.0]"),
            "return-special" => Some("[-0.0 ##NaN ##Inf 9223372036854775807 -9223372036854775808 0 :error :error]"),
            "cast-order" => Some("[:error [:first :second]]"),
            "redefinition" => Some("[[:changed 3] 3.5]"),
            "return-names" => Some("[1 2.0 5.0 2 4.0 1.75 3.0]"),
            _ => None,
        };
        if let Some(expected) = expected {
            assert_eq!(aot, expected, "{name}: {source}");
        }
    }
}

#[test]
fn wit_parameter_specialization_does_not_change_source_return_typing() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("app.sus"),
        "(ns app) (defn as-long ^long [x] x) (defn as-double ^double [x] x)").unwrap();
    std::fs::write(root.path().join("app.wit"),
        "package test:source-tags; world w { export as-long: func(x: f64) -> s64; export as-double: func(x: s64) -> f64; }").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(root.path())
        .args(["compile", "app.sus", "-w", "app.wit", "-o", "app.wasm",
               "--export", "as-long=app/as-long", "--export", "as-double=app/as-double"])
        .output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    for (name, argument, expected) in [("as-long", "1.75", 1.0_f64), ("as-double", "3", 3.0)] {
        let output = Command::new(env!("CARGO_BIN_EXE_suss"))
            .current_dir(root.path())
            .args(["run", "app.wasm", "--invoke", name, "--", argument])
            .output().unwrap();
        assert!(output.status.success(), "{name}: {}", String::from_utf8_lossy(&output.stderr));
        let value: f64 = String::from_utf8_lossy(&output.stdout).trim().parse().unwrap();
        assert_eq!(value.to_bits(), expected.to_bits(), "{name}");
    }
}
