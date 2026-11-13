//! A closure call site caches its last callee and that callee's entry (#378,
//! runtime ABI 6). One site called with many kinds of callee, in turn and
//! repeatedly, must still call each correctly: closures of one or several
//! arities, variadic closures, keywords, maps, sets, and nil, whose call
//! raises. Each case's AOT result must equal the REPL's. Operands derive from
//! the argument `n`, 0.
use std::process::Command;
use suss_cli::portable_session::Session;

const DEFINITIONS: &str = "(def call1 (fn [f x] (f x)))
(def call2 (fn [f x y] (f x y)))
(def tail1 (fn [f x] (if (nil? x) :none (f x))))
(def inc2 (fn [x] (+ x 2)))
(def multi (fn ([x] [:one x]) ([x y] [:two x y])))
(def rest-fn (fn [& xs] (vec xs)))
(def callees (fn [n] [inc2 (fn [x] (* x 3)) multi rest-fn :k {(+ n 1) :map} #{(+ n 1)} inc2 multi]))";
const CASES: &[(&str, &str)] = &[
    ("one-arg", "(mapv (fn [f] (call1 f (+ n 1))) (callees n))"),
    ("one-arg-twice", "(vec (concat (mapv (fn [f] (call1 f (+ n 1))) (callees n)) (mapv (fn [f] (call1 f {:k n})) [:k :k multi :k])))"),
    ("two-args", "(mapv (fn [f] (call2 f (+ n 1) 5)) [+ multi rest-fn {} :k max])"),
    ("tail", "(mapv (fn [f] (tail1 f (+ n 4))) [inc2 multi rest-fn inc2])"),
    ("nil-callee", "(try (call1 nil n) (catch :default e :not-callable))"),
    ("after-nil", "[(try (call1 nil n) (catch :default e :not-callable)) (call1 inc2 n) (try (call1 nil n) (catch :default e :not-callable))]"),
    ("closure-captures", "(mapv (fn [k] (call1 (fn [x] (+ x k)) n)) (range 4))"),
    ("arity-error", "[(call1 multi n) (try (call1 (fn [] :zero) n) (catch :default e :arity)) (call1 multi n)]"),
];

#[test]
fn closure_call_sites_match_the_repl_for_changing_callees() {
    let root = tempfile::tempdir().unwrap();
    let mut app = format!("(ns app)\n{DEFINITIONS}\n");
    for (name, source) in CASES {
        app.push_str(&format!("(defn case-{name} [n] (pr-str {source}))\n"));
    }
    std::fs::write(root.path().join("app.sus"), &app).unwrap();
    let exports = CASES.iter().map(|(name, _)| format!("export case-{name}: func(n: u32) -> string;")).collect::<Vec<_>>();
    std::fs::write(root.path().join("app.wit"), format!("package test:calls; world w {{ {} }}", exports.join(" "))).unwrap();
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
        // The expected values, independent of either implementation.
        let expected = match *name {
            "nil-callee" => Some(":not-callable"),
            "after-nil" => Some("[:not-callable 2 :not-callable]"),
            "arity-error" => Some("[[:one 0] :arity [:one 0]]"),
            "closure-captures" => Some("[0 1 2 3]"),
            "tail" => Some("[6 [:one 4] [4] 6]"),
            _ => None,
        };
        if let Some(expected) = expected {
            assert_eq!(aot, expected, "{name}: {source}");
        }
    }
}
