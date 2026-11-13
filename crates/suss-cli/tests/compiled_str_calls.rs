//! A direct-linked `(str a b …)` with 2 to 8 operands concatenates each
//! operand's `(str x)` in order without str's rest seq (#388); a long after
//! the first operand has its digits appended directly. Each case's
//! AOT result must equal the REPL's, which calls str, and some fixed expected
//! values. Operands derive from the argument `n`, 0.
use std::process::Command;
use suss_cli::portable_session::Session;

const DEFINITIONS: &str = "(def log (atom []))
(def tracked (fn [tag v] (swap! log conj [:eval tag]) (js-obj \"toString\" (fn [] (swap! log conj [:str tag]) v))))
(def boom (fn [tag] (js-obj \"toString\" (fn [] (swap! log conj [:str tag]) (throw (ex-info \"boom\" {:tag tag}))))))";
const CASES: &[(&str, &str)] = &[
    ("two", "[(str \"a\" n) (str n \"a\") (str nil n) (str n nil) (str :k 'sym)]"),
    ("three", "[(str \"a\" n \"b\") (str nil nil nil) (str 1.5 (+ n 7) true)]"),
    ("many", "[(str 1 2 3 4) (str 1 2 3 4 5) (str 1 2 3 4 5 6 7 8) (str 1 2 3 4 5 6 7 8 9)]"),
    ("values", "(str [n 2] {:a n} #{3} '(4 5) (range 3) \"λ🙂\")"),
    ("longs", "(str (+ n 3000000000) \"/\" (- n 1073741825) \"/\" (+ n 9223372036854775807))"),
    ("order", "(do (reset! log []) [(str (tracked 1 \"a\") (tracked 2 \"b\") (tracked 3 \"c\")) @log])"),
    ("throw", "(do (reset! log []) [(try (str (tracked 1 \"a\") (boom 2) (tracked 3 \"c\")) (catch :default e (:tag (ex-data e)))) @log])"),
    ("append-longs", "[(str \"\" n) (str \"x\" (- n 9223372036854775807 1)) (str \"a\" 0 -1 (+ n 7)) (str nil (+ n 1073741824)) (str \"λ\" (- n 12345))]"),
    ("nested", "(str (str \"a\" n) (str) (str (str n n) \"z\"))"),
];

#[test]
fn direct_linked_str_calls_match_the_repl() {
    let root = tempfile::tempdir().unwrap();
    let mut app = format!("(ns app)\n{DEFINITIONS}\n");
    for (name, source) in CASES {
        app.push_str(&format!("(defn case-{name} [n] (pr-str {source}))\n"));
    }
    std::fs::write(root.path().join("app.sus"), &app).unwrap();
    let exports = CASES.iter().map(|(name, _)| format!("export case-{name}: func(n: u32) -> string;")).collect::<Vec<_>>();
    std::fs::write(root.path().join("app.wit"), format!("package test:strs; world w {{ {} }}", exports.join(" "))).unwrap();
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
            "two" => Some(r#"["a0" "0a" "0" "0" ":ksym"]"#),
            "append-longs" => Some(r#"["0" "x-9223372036854775808" "a0-17" "1073741824" "λ-12345"]"#),
            "many" => Some(r#"["1234" "12345" "12345678" "123456789"]"#),
            "order" => Some("[\"abc\" [[:eval 1] [:eval 2] [:eval 3] [:str 1] [:str 2] [:str 3]]]"),
            "throw" => Some("[2 [[:eval 1] [:eval 3] [:str 1] [:str 2]]]"),
            _ => None,
        };
        if let Some(expected) = expected {
            assert_eq!(aot, expected, "{name}: {source}");
        }
    }
}
