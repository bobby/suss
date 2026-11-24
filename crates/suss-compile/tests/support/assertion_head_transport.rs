//! A transport spelling for reserved core namespaces, with the raw source
//! head retained separately in every observation. Production resolution maps
//! clojure.core, cljs.core and suss.core to the same core binding and forbids
//! redirecting the latter two through aliases (portable/resolve.rs).
//!
//! Syntax quote consequently emits cljs.core/= where the JVM emits
//! clojure.core/=. This adapter changes only that namespace spelling. It never
//! consults expected results, changes an unqualified/user head, or touches
//! execution, verdicts or independently captured operand values.
pub(super) fn canonical(head: &str) -> String {
    match head.split_once('/') {
        Some(("cljs.core" | "suss.core", name)) => format!("clojure.core/{name}"),
        _ => head.to_owned(),
    }
}

#[test]
fn assertion_head_transport_keeps_user_heads_distinct() {
    for head in ["=", "custom/=", "p/thrown?", "cljs.core.extra/=", "clojure.core/="] {
        assert_eq!(canonical(head), head);
    }
    assert_eq!(canonical("cljs.core/="), "clojure.core/=");
    assert_eq!(canonical("suss.core/not="), "clojure.core/not=");
}

#[test]
fn assertion_head_transport_executes_local_macro_and_preserves_comparison_guards() {
    use super::*;
    let workspace = tempfile::tempdir().unwrap();
    let source_root = workspace.path().join("src");
    let generated = Command::new("python3")
        .arg(root().join("scripts/clojure_test_suite.py"))
        .args(["suss-sources", "--suite", "fixture", "--output"])
        .arg(&source_root).output().unwrap();
    assert!(generated.status.success());
    let source = source_file(&source_root, "head.control");
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    std::fs::write(source, r#"
      (ns head.control
        (:require [clojure.test :refer [deftest is]]))
      (defmacro same [a b] `(suss.harness.test-macros/is (= ~a ~b)))
      (def custom-equal (fn [a b] (clojure.core/= a b)))
      (deftest heads
          (same 17 17)
          (is (cljs.core/= 17 17))
          (is (suss.core/= 17 17))
          (is (clojure.core/= 17 17))
          (is (head.control/custom-equal 17 17))
          (is (identical? cljs.core/= clojure.core/=))
          (is (identical? suss.core/= clojure.core/=)))
    "#).unwrap();
    let observations = Harness::new(&source_root).run(
        &json!({"namespaces":["head.control"],"tests":["head.control/heads"]}),
        &source_root,
    );
    assert_eq!(observations["namespaces"], json!([{"namespace":"head.control","status":"loaded"}]));
    assert_eq!(observations["test-failures"], json!([]));
    assert_eq!(observations["skips"], json!([]));
    let captured = observations["assertions"].as_array().unwrap();
    assert_eq!(captured.len(), 7);
    assert_eq!(captured[0]["source-head"], "cljs.core/=");
    for (row, raw) in captured[..4].iter().zip([
        "cljs.core/=", "cljs.core/=", "suss.core/=", "clojure.core/=",
    ]) {
        assert_eq!(row["head"], "clojure.core/=");
        assert_eq!(row["source-head"], raw);
        assert_eq!(row["operands"], json!([
            {"tag":"long","value":"17"}, {"tag":"long","value":"17"},
        ]));
    }
    assert_eq!(captured[4]["head"], "head.control/custom-equal");
    assert_eq!(captured[4]["source-head"], "head.control/custom-equal");
    for row in &captured[5..] {
        assert_eq!(row["verdict"], "pass");
        assert_eq!(row["result"], json!({"tag":"bool","value":true}));
    }
    let capture = workspace.path().join("heads.json");
    std::fs::write(&capture, serde_json::to_vec(&captured).unwrap()).unwrap();
    let compared = Command::new("python3").arg("-c").arg(r#"
import copy,json,sys
from pathlib import Path
sys.path.insert(0,str(Path(sys.argv[1])/'scripts'))
import clojure_test_suite as suite
rows=json.loads(Path(sys.argv[2]).read_text())
expected={'id':'head.control/heads#1','deterministic':True,
          'kind':'predicate','form':'(clojure.core/= 17 17)','verdict':'pass',
          'operands':[{'tag':'long','value':'17'},{'tag':'long','value':'17'}]}
for row in rows[:4]: assert suite.decide(expected,row)==(None,False)
assert suite.decide(expected,rows[4])[0].startswith('head:')
for field,value in [('head','clojure.core/not='),('verdict','fail'),
                    ('operands',[{'tag':'long','value':'17'},{'tag':'long','value':'18'}])]:
    altered=copy.deepcopy(rows[0]);altered[field]=value
    assert suite.decide(expected,altered)[0] is not None,field
print('4 independently decoded core-alias assertions; user head, wrong operator, verdict and operands rejected')
"#).arg(root()).arg(capture).output().unwrap();
    assert!(compared.status.success(), "{}\n{}", String::from_utf8_lossy(&compared.stdout),
        String::from_utf8_lossy(&compared.stderr));
    println!("{}", String::from_utf8_lossy(&compared.stdout));
}
