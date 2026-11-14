//! A WIT export's numeric parameters are primitive in its fn (#385): integer
//! WIT types are `^long` and floats `^double`, so loops seeded from them stay
//! raw beyond i31. Calls through the WIT boundary pass numbers of the declared
//! kind; an internal call coerces as Clojure's primitive fns do. A nested fn
//! inside an export takes no hints.
use std::process::Command;

const SOURCE: &str = r#"(ns app)
(defn count-up [n m] (loop [i n k 0] (if (< k m) (recur (inc i) (inc k)) i)))
(defn wide [n] (+ n 1))
(defn half [x] (/ x 2))
(defn arities ([n] (* n 10)) ([n m] (+ n m)))
(defn nested [n] ((fn [x] (str x)) (/ n 2)))
(defn internal [n] (pr-str [(count-up 2.5 1) (half 3) (try (count-up "x" 1) (catch :default e :error))]))
"#;
const WIT: &str = "package test:hints; world w {
  export count-up: func(n: u32, m: u32) -> u64;
  export wide: func(n: s64) -> s64;
  export half: func(x: f64) -> f64;
  export arities: func(n: u32, m: u32) -> u32;
  export nested: func(n: u32) -> string;
  export internal: func(n: u32) -> string;
}";

fn run(dir: &std::path::Path, name: &str, args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(dir)
        .args(["run", "app.wasm", "--invoke", name, "--"])
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "{name}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

#[test]
fn wit_export_numeric_parameters_are_primitive() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("app.sus"), SOURCE).unwrap();
    std::fs::write(root.path().join("app.wit"), WIT).unwrap();
    let mut compile = Command::new(env!("CARGO_BIN_EXE_suss"));
    compile.current_dir(root.path()).args(["compile", "app.sus", "-w", "app.wit", "-o", "app.wasm"]);
    for name in ["count-up", "wide", "half", "arities", "nested", "internal"] {
        compile.args(["--export", &format!("{name}=app/{name}")]);
    }
    let output = compile.output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let dir = root.path();
    assert_eq!(run(dir, "count-up", &["3000000000", "5"]), "3000000005");
    assert_eq!(run(dir, "count-up", &["1073741820", "10"]), "1073741830");
    assert_eq!(run(dir, "wide", &["9223372036854775806"]), "9223372036854775807");
    assert_eq!(run(dir, "wide", &["-5"]), "-4");
    assert_eq!(run(dir, "half", &["3"]), "1.5");
    assert_eq!(run(dir, "arities", &["4", "5"]), "9");
    // The nested fn isn't an export and takes no hint: str prints its
    // argument, (/ n 2), unconverted.
    assert_eq!(run(dir, "nested", &["7"]), "3.5");
    // Internal calls coerce as Clojure's primitive fns: a double truncates
    // for an integer export (2.5 counts up once from 2), a long widens for a
    // float export, and a non-number throws.
    assert_eq!(run(dir, "internal", &["0"]), "[3 1.5 :error]");
}
