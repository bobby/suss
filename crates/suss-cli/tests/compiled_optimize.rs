//! `suss compile --optimize` merges a component's core modules and optimizes
//! them with Binaryen (ADR-0005, #357). The optimized component gives the same
//! results as the unoptimized one, including direct-linked calls between user
//! functions (#360). Skipped when no Binaryen is installed.
use std::process::Command;

const APP: &str = r#"(ns app)
(defprotocol Shape (area [s]))
(deftype Square [n] Shape (area [_] (* n n)))
(defn arith [n] (+ (* n 3) (quot n 2)))
(defn keywords [n] (if (= :a (keyword (if (pos? n) "a" "b"))) 1 0))
(defn vectors [n] (reduce + (map inc (vec (range n)))))
(defn strings [n] (count (apply str (repeat n "ab"))))
(defn protocols [n] (area (Square. n)))
(defn closures [n] (let [add (fn [x] (fn [y] (+ x y)))] ((add n) 10)))
(defn errors [n] (try (if (pos? n) (throw (ex-info "boom" {:n n})) 0) (catch :default e (:n (ex-data e)))))
(defn maps [n] (get (zipmap (range n) (range 100 (+ 100 n))) (dec n) -1))
(defn helper [x] (* 2 x))
(defn linked [n] (+ (helper n) (helper (inc n))))
"#;
const CASES: &[&str] = &[
    "arith",
    "keywords",
    "vectors",
    "strings",
    "protocols",
    "closures",
    "errors",
    "maps",
    "linked",
];

fn binaryen_installed() -> bool {
    let tool = std::env::var_os("SUSS_WASM_OPT").unwrap_or_else(|| "wasm-opt".into());
    Command::new(tool)
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

#[test]
fn optimized_components_match_unoptimized_ones() {
    if !binaryen_installed() {
        eprintln!("skipped: no wasm-opt");
        return;
    }
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("app.sus"), APP).unwrap();
    let exports = CASES
        .iter()
        .map(|name| format!("export {name}: func(n: s64) -> s64;"))
        .collect::<Vec<_>>();
    std::fs::write(
        root.path().join("app.wit"),
        format!("package test:opt; world w {{ {} }}", exports.join(" ")),
    )
    .unwrap();
    let mut sizes = vec![];
    for (output, optimize) in [("plain.wasm", false), ("optimized.wasm", true)] {
        let mut compile = Command::new(env!("CARGO_BIN_EXE_suss"));
        compile.current_dir(root.path()).args([
            "--no-cache",
            "compile",
            "app.sus",
            "-w",
            "app.wit",
            "-o",
            output,
        ]);
        for name in CASES {
            compile.args(["--export", &format!("{name}=app/{name}")]);
        }
        if optimize {
            compile.arg("--optimize");
        }
        let result = compile.output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        sizes.push(std::fs::metadata(root.path().join(output)).unwrap().len());
    }
    assert!(
        sizes[1] < sizes[0],
        "optimized {} bytes, unoptimized {}",
        sizes[1],
        sizes[0]
    );
    for name in CASES {
        for n in ["0", "5"] {
            let run = |component: &str| {
                let output = Command::new(env!("CARGO_BIN_EXE_suss"))
                    .current_dir(root.path())
                    .args(["run", component, "--invoke", name, "--", n])
                    .output()
                    .unwrap();
                (
                    output.status.success(),
                    String::from_utf8_lossy(&output.stdout).trim().to_owned(),
                )
            };
            let plain = run("plain.wasm");
            assert!(plain.0, "{name} {n} failed unoptimized");
            assert_eq!(run("optimized.wasm"), plain, "{name} {n}");
        }
    }
}

#[test]
fn optimize_rejects_command_components() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("main.sus"),
        "(ns main) (defn -main [& args] 0)",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(root.path())
        .args([
            "--no-cache",
            "compile",
            "main.sus",
            "-m",
            "main",
            "-o",
            "main.wasm",
            "--optimize",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--optimize doesn't support --main"));
}
