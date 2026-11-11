//! AOT builds direct-link user code (#360), as JVM Clojure 1.12.6 does with
//! `-Dclojure.compiler.direct-linking=true`: a call of a var defined as a
//! function calls that function directly, so `with-redefs` doesn't reach it,
//! while a later `defn` in the same build does, as the JVM loads the newest
//! class of the same name. `^:redef` and `^:dynamic` vars stay live. The REPL
//! doesn't direct-link, so `with-redefs` reaches every call there, as in JVM
//! Clojure without the flag.
use std::process::{Command, Stdio};
use std::io::Write;

const LIB: &str = "(ns lib)
(defn once [x] x)
(defn twice [x] (+ (once x) (once x)))";
const DEFINITIONS: &str = "(defn f [] 1)
(defn g [] (f))
(defn ^:redef r [] 1)
(defn gr [] (r))
(defn ^:dynamic d [] 1)
(defn gd [] (d))
(defn value-use [] (first (map (fn [h] (h)) [f])))
(defn f [] 2)
(defn r [] 2)
(defn self-count [n] (if (pos? n) (self-count (dec n)) 1))
(defn g-after [] (f))";
/// Case, source, JVM Clojure's result with direct linking.
const CASES: &[(&str, &str, i64)] = &[
    ("direct", "(g)", 2),
    ("redef", "(gr)", 2),
    ("dynamic", "(binding [d (fn [] 3)] (gd))", 3),
    ("with-redefs-direct", "(with-redefs [f (fn [] 4)] (g))", 2),
    ("with-redefs-redef", "(with-redefs [r (fn [] 4)] (gr))", 4),
    ("value", "(value-use)", 2),
    ("after", "(g-after)", 2),
    ("self", "(self-count 1000)", 1),
    ("lib", "(lib/twice 21)", 42),
];

#[test]
fn aot_builds_direct_link_user_calls_as_jvm_clojure_does() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("lib.sus"), LIB).unwrap();
    let mut app = format!("(ns app (:require [lib]))\n{DEFINITIONS}\n");
    for (name, source, _) in CASES {
        app.push_str(&format!("(defn case-{name} [n] {source})\n"));
    }
    std::fs::write(root.path().join("app.sus"), app).unwrap();
    let exports = CASES.iter().map(|(name, ..)| format!("export case-{name}: func(n: u32) -> s64;")).collect::<Vec<_>>();
    std::fs::write(root.path().join("app.wit"), format!("package test:dl; world w {{ {} }}", exports.join(" "))).unwrap();
    let mut compile = Command::new(env!("CARGO_BIN_EXE_suss"));
    compile.current_dir(root.path()).args(["compile", "app.sus", "-w", "app.wit", "-o", "app.wasm", "--src", "."]);
    for (name, ..) in CASES {
        compile.args(["--export", &format!("case-{name}=app/case-{name}")]);
    }
    let output = compile.output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    // Calls between user functions import each other's entries.
    let component = std::fs::read(root.path().join("app.wasm")).unwrap();
    assert!(component.windows(10).any(|bytes| bytes == b"suss.slot."), "no direct links");
    for (name, source, expected) in CASES {
        let output = Command::new(env!("CARGO_BIN_EXE_suss"))
            .current_dir(root.path())
            .args(["run", "app.wasm", "--invoke", &format!("case-{name}"), "--", "0"])
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(output.status.success(), "{source}: {}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(stdout.trim(), expected.to_string(), "{source}");
    }
}

#[test]
fn the_repl_keeps_live_calls() {
    let mut repl = Command::new(env!("CARGO_BIN_EXE_suss"))
        .arg("repl")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let input = format!(
        "{DEFINITIONS}\n(with-redefs [f (fn [] 4)] (g))\n(g)\n"
    );
    repl.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    let output = repl.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines = stdout.lines().collect::<Vec<_>>();
    // JVM Clojure without direct linking: 4 under with-redefs, then 2.
    assert_eq!(&lines[lines.len() - 2..], ["4", "2"], "{stdout}");
}

/// `(= :k y)` compiles to an identity test in AOT builds (#361), as core's
/// `=` answers for a keyword first operand; a keyword second operand still
/// asks the first. JVM Clojure 1.12.6 gives the same results, with a type
/// whose `equals` is always true in place of `Eq`.
const KEYWORD_CASES: &[(&str, &str, i64)] = &[
    ("same", "(= :a :a)", 1),
    ("different", "(= :a :b)", 0),
    ("built", "(= :a (keyword \"a\"))", 1),
    ("string", "(= :a \"a\")", 0),
    ("nil", "(= :a nil)", 0),
    ("qualified", "(= :ns/a :ns/a)", 1),
    ("keyword-first", "(= :a (Eq.))", 0),
    ("keyword-second", "(= (Eq.) :a)", 1),
];

#[test]
fn aot_keyword_equality_matches_core() {
    let root = tempfile::tempdir().unwrap();
    let mut app = String::from("(ns app)\n(deftype Eq [] IEquiv (-equiv [_ _] true))\n");
    for (name, source, _) in KEYWORD_CASES {
        app.push_str(&format!("(defn case-{name} [n] (if {source} 1 0))\n"));
    }
    std::fs::write(root.path().join("app.sus"), app).unwrap();
    let exports = KEYWORD_CASES.iter().map(|(name, ..)| format!("export case-{name}: func(n: u32) -> s64;")).collect::<Vec<_>>();
    std::fs::write(root.path().join("app.wit"), format!("package test:kw; world w {{ {} }}", exports.join(" "))).unwrap();
    let mut compile = Command::new(env!("CARGO_BIN_EXE_suss"));
    compile.current_dir(root.path()).args(["compile", "app.sus", "-w", "app.wit", "-o", "app.wasm", "--src", "."]);
    for (name, ..) in KEYWORD_CASES {
        compile.args(["--export", &format!("case-{name}=app/case-{name}")]);
    }
    let output = compile.output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    for (name, source, expected) in KEYWORD_CASES {
        let output = Command::new(env!("CARGO_BIN_EXE_suss"))
            .current_dir(root.path())
            .args(["run", "app.wasm", "--invoke", &format!("case-{name}"), "--", "0"])
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(output.status.success(), "{source}: {}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(stdout.trim(), expected.to_string(), "{source}");
    }
}
