//! A REPL started without budget flags runs ordinary programs: definitions
//! whose macro expansion is expensive, and collection work of a few hundred
//! thousand steps (#289). An explicit limit reports which budget ran out and
//! the flag that raises it.
use std::io::Write;
use std::process::{Command, Stdio};

fn repl(flags: &[&str], source: &str) -> (String, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_suss"))
        .arg("repl")
        .args(flags)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(source.as_bytes()).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:?}");
    (String::from_utf8(output.stdout).unwrap(), String::from_utf8(output.stderr).unwrap())
}

#[test]
fn default_repl_budgets_run_records_protocols_and_large_collections() {
    let (out, errors) = repl(&[], concat!(
        "(defrecord Point [x y])\n",
        "(:y (->Point 1 2))\n",
        "(defprotocol Area (area [s]))\n",
        "(extend-type Point Area (area [p] (* (:x p) (:y p))))\n",
        "(area (->Point 3 4))\n",
        "(deftype Box [v] Area (area [_] v))\n",
        "(area (Box. 9))\n",
        "(let [v (vec (range 100000))] [(count v) (nth v 99999) (reduce + v)])\n",
    ));
    assert_eq!(errors, "");
    let lines = out.lines().collect::<Vec<_>>();
    assert_eq!(lines[1], "2", "{out}");
    assert_eq!(lines[4], "12", "{out}");
    assert_eq!(lines[6], "9", "{out}");
    assert_eq!(lines[7], "[100000 99999 4999950000]", "{out}");
}

#[test]
fn explicit_repl_budgets_name_the_exhausted_budget_and_its_flag() {
    let (out, errors) = repl(&["--fuel", "1000000"], concat!(
        "(count (vec (range 100000)))\n",
        "(+ 1 2)\n",
    ));
    assert_eq!(out, "3\n");
    assert!(errors.contains("Evaluation fuel exhausted (limit 1000000 per input); raise it with --fuel"), "{errors}");
    let (out, errors) = repl(&["--compile-fuel", "1000000"], concat!(
        "(defrecord Point [x y])\n",
        "(+ 1 2)\n",
    ));
    assert_eq!(out, "3\n");
    assert!(errors.contains("Compilation fuel exhausted (limit 1000000 per input); raise it with --compile-fuel"), "{errors}");
    // A runtime trap after a failed compilation is still evaluation fuel.
    let (out, errors) = repl(&["--fuel", "1000000"], concat!(
        "(defrecord Point [x y])\n",
        "(count (vec (range 100000)))\n",
        "(+ 1 2)\n",
    ));
    assert_eq!(out, "3\n");
    let errors = errors.lines().collect::<Vec<_>>();
    assert!(errors[0].starts_with("Error: Compilation fuel exhausted (limit 1000000 per input)"), "{errors:?}");
    assert!(errors[1].starts_with("Error: Evaluation fuel exhausted (limit 1000000 per input)"), "{errors:?}");
    // A macro definition traps in the macro session: compilation fuel, not
    // the (ample) evaluation budget.
    let (_, errors) = repl(&["--fuel", "100000000000", "--compile-fuel", "3000"], "(defmacro m [] 1)\n");
    assert!(errors.starts_with("Error: Compilation fuel exhausted (limit 3000 per input); raise it with --compile-fuel"), "{errors}");
}

/// Unbounded budgets never claim exhaustion, and a compiler-internal fixed
/// counter is not reported as a budget the flags could raise.
#[test]
fn unbounded_budgets_report_real_errors() {
    let (out, errors) = repl(&[], "(throw (ex-info \"fuel exhausted\" {}))\n(+ 1 2)\n");
    assert_eq!(out, "3\n");
    assert!(!errors.contains("raise it with"), "{errors}");
}
