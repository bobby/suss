//! Complete original eval body, including all three remaining numeric rows.
use super::*;

fn check_body(stem: &str, namespace: &str, count: usize) {
    let repository = root();
    let workspace = tempfile::tempdir().unwrap();
    let source_root = workspace.path().join("src");
    let generated = Command::new("python3").arg(repository.join("scripts/clojure_test_suite.py"))
        .args(["suss-sources", "--suite", "suite", "--output"])
        .arg(&source_root).output().unwrap();
    assert!(generated.status.success(), "{}", String::from_utf8_lossy(&generated.stderr));
    let original = repository.join(format!(
        "vendor/clojure-test-suite/upstream/test/clojure/core_test/{stem}.cljc"));
    assert_eq!(std::fs::read(original).unwrap(), std::fs::read(source_file(&source_root, namespace)).unwrap());
    let test = format!("{namespace}/test-{}", namespace.rsplit('.').next().unwrap());
    let observed = Harness::new(&source_root).run(
        &json!({"namespaces":[namespace],"tests":[test]}), &source_root);
    let captured = workspace.path().join("observations.json");
    std::fs::write(&captured, serde_json::to_vec_pretty(&observed).unwrap()).unwrap();
    if let Some(directory) = std::env::var_os("SUSS_EVAL_BODY_CAPTURE") {
        let directory = PathBuf::from(directory);std::fs::create_dir_all(&directory).unwrap();
        std::fs::copy(&captured, directory.join(format!("{stem}-observations.json"))).unwrap();
    }
    let compared = Command::new("python3").arg("-c").arg(r#"
import hashlib,json,sys
from pathlib import Path
root=Path(sys.argv[1]);sys.path.insert(0,str(root/'scripts'))
import clojure_test_suite as suite
raw=(root/'tests/oracle/clojure-test-suite-observations.json').read_bytes()
assert hashlib.sha256(raw).hexdigest()=='449264ab6bca1dcf761d0204e4a64de4de91562e576615c4e84a63246839f4c7'
reference=json.loads(raw);namespace=sys.argv[3];count=int(sys.argv[4])
test=namespace+'/test-'+namespace.rsplit('.',1)[1]
reference.update(namespaces=[namespace],tests=[test],
 assertions=[r for r in reference['assertions'] if r['test']==test],
 skips=[r for r in reference['skips'] if r['namespace']==namespace])
observed=json.loads(Path(sys.argv[2]).read_text());counts,failures=suite.compare(reference,observed)
wanted=[(r['test'],r['ordinal']) for r in reference['assertions']]
actual=[(r['test'],r['ordinal']) for r in observed['assertions']]
assert wanted==actual==[(test,n) for n in range(1,count+1)]
assert observed['namespaces']==[{'namespace':namespace,'status':'loaded'}]
assert not observed['test-failures'] and not observed['skips'] and not reference['skips']
assert not any(failures[k] for k in ['tests','namespaces','assertions','skip-mismatches'])
guests=[w['ordinal'] for w,a in zip(reference['assertions'],observed['assertions']) if suite.decide(w,a)[1]]
# Preserve all five opaque function/Var judgments from the original primary.
# The three exact numeric rows are independently comparable host observations.
assert guests==[15,16,17,18,19]
assert counts['pass']==29 and counts['guest-judged']==5
assert counts['fail']==counts['not-executed']==counts['skipped']==0
for ordinal in [5,6,7]:
 assert suite.decide(reference['assertions'][ordinal-1],observed['assertions'][ordinal-1])==(None,False)
print(json.dumps({'counts':counts,'guest_ordinals':guests,'ordered_ids_match':True}))
"#).arg(repository).arg(captured).arg(namespace).arg(count.to_string()).output().unwrap();
    assert!(compared.status.success(), "{}\n{}", String::from_utf8_lossy(&compared.stdout),
        String::from_utf8_lossy(&compared.stderr));
    println!("{}", String::from_utf8_lossy(&compared.stdout));
}

#[test]
fn complete_eval_body_keeps_exact_numeric_fields_as_host_matches() {
    check_body("eval", "clojure.core-test.eval", 34);
}
