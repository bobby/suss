//! Protocol method calls in AOT builds dispatch at their own call site (#378),
//! as Clojure's compiler emits a class test and an interface call at each
//! protocol call site: an object whose type implements the method calls it
//! through the site's cached lookup, and anything else calls the protocol fn.
//! Each case's AOT result must equal the REPL's, where calls go through the
//! protocol fn's dispatcher. Operands derive from the argument `n`, 0.
use std::process::Command;
use suss_cli::portable_session::Session;

const DEFINITIONS: &str = "(defprotocol Shape (area [s]) (scaled [s k]))
(defprotocol Named (label [x]))
(deftype Sq [w] Shape (area [_] (* w w)) (scaled [_ k] (Sq. (* w k))))
(deftype Rect [w h] Shape (area [_] (* w h)) (scaled [_ k] (->Rect (* w k) (* h k))))
(deftype Circle [r])
(deftype Boom [] Shape (area [_] (throw (ex-info \"boom\" {:at :area}))) (scaled [this _] this))
(extend-type nil Named (label [_] \"nil\"))
(extend-type number Named (label [x] (str \"n\" x)))
(extend-type string Named (label [x] (str \"s\" x)))
(extend-type Sq Named (label [_] \"square\"))
(def shapes (fn [n] [(Sq. (+ n 3)) (->Rect (+ n 2) 5) (reify Shape (area [_] 7) (scaled [this _] this)) (Sq. (+ n 1))]))
(def total (fn [n] (reduce + (map area (shapes n)))))
(def each-label (fn [n] (apply str (map label [nil (+ n 1) \"x\" (Sq. n)]))))
(def missing (fn [n] (try (area (Circle. n)) (catch :default e :no-impl))))
(def missing-native (fn [n] (try (area (+ n 1)) (catch :default e :no-impl))))
(def later (fn [n] (extend-type Circle Shape (area [c] (* 3 (.-r c) (.-r c))) (scaled [c k] (Circle. (* k (.-r c))))) (area (Circle. (+ n 2)))))";
/// Case name and an expression of `n` whose value prints as a string.
const CASES: &[(&str, &str)] = &[
    ("inline", "(area (Sq. (+ n 4)))"),
    // defrecord in an AOT build exceeds the default macro budget (#311).
    ("two-fields", "(area (->Rect (+ n 2) 3))"),
    ("arity-two", "(area (scaled (Sq. (+ n 2)) 3))"),
    ("polymorphic", "(total n)"),
    ("polymorphic-repeat", "(+ (total n) (total n) (total n))"),
    ("reify", "(area (reify Shape (area [_] (+ n 11)) (scaled [this _] this)))"),
    ("native-and-nil", "(each-label n)"),
    ("missing", "(missing n)"),
    ("missing-native", "(missing-native n)"),
    ("throws", "(try (area (Boom.)) (catch :default e (ex-data e)))"),
    ("extended-later", "[(missing n) (later n) (area (Circle. (+ n 1)))]"),
    ("satisfies", "[(satisfies? Shape (Sq. n)) (satisfies? Shape n) (satisfies? Named n)]"),
];

#[test]
fn protocol_call_sites_match_the_dispatcher() {
    let root = tempfile::tempdir().unwrap();
    let mut app = format!("(ns app)\n{DEFINITIONS}\n");
    for (name, source) in CASES {
        app.push_str(&format!("(defn case-{name} [n] (pr-str {source}))\n"));
    }
    std::fs::write(root.path().join("app.sus"), &app).unwrap();
    let exports = CASES.iter().map(|(name, _)| format!("export case-{name}: func(n: u32) -> string;")).collect::<Vec<_>>();
    std::fs::write(root.path().join("app.wit"), format!("package test:proto; world w {{ {} }}", exports.join(" "))).unwrap();
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
    }
}
