//! Execute the complete unchanged vendored body with the ordinary harness.
use super::*;

#[test]
fn all_min_key_body_assertions_match_the_pinned_primary() {
    let repository = root();
    let workspace = tempfile::tempdir().unwrap();
    let source_root = workspace.path().join("src");
    let generated = Command::new("python3")
        .arg(repository.join("scripts/clojure_test_suite.py"))
        .args(["suss-sources", "--suite", "suite", "--output"])
        .arg(&source_root).output().unwrap();
    assert!(generated.status.success(), "{}", String::from_utf8_lossy(&generated.stderr));
    let namespace = "clojure.core-test.min-key";
    let original = repository.join("vendor/clojure-test-suite/upstream/test/clojure/core_test/min_key.cljc");
    assert_eq!(std::fs::read(original).unwrap(), std::fs::read(source_file(&source_root, namespace)).unwrap());
    let observations = Harness::new(&source_root).run(
        &json!({"namespaces":[namespace],"tests":["clojure.core-test.min-key/test-min-key"]}), &source_root);
    let captured = workspace.path().join("min-key-observations.json");
    std::fs::write(&captured, serde_json::to_vec_pretty(&observations).unwrap()).unwrap();
    if let Some(directory) = std::env::var_os("SUSS_MIN_KEY_BODY_CAPTURE") {
        let directory = PathBuf::from(directory);std::fs::create_dir_all(&directory).unwrap();
        std::fs::copy(&captured, directory.join("min-key-observations.json")).unwrap();
    }
    let compared = Command::new("python3").arg("-c").arg(r#"
import hashlib,json,sys
from pathlib import Path
root=Path(sys.argv[1]);sys.path.insert(0,str(root/'scripts'))
import clojure_test_suite as suite
raw=(root/'tests/oracle/clojure-test-suite-observations.json').read_bytes()
assert hashlib.sha256(raw).hexdigest()=='449264ab6bca1dcf761d0204e4a64de4de91562e576615c4e84a63246839f4c7'
reference=json.loads(raw);namespace='clojure.core-test.min-key';test=namespace+'/test-min-key'
reference.update(namespaces=[namespace],tests=[test],
 assertions=[x for x in reference['assertions'] if x['test']==test],
 skips=[x for x in reference['skips'] if x['namespace']==namespace])
observed=json.loads(Path(sys.argv[2]).read_text());counts,failures=suite.compare(reference,observed)
expected=[(x['test'],x['ordinal']) for x in reference['assertions']]
actual=[(x['test'],x['ordinal']) for x in observed['assertions']]
print(json.dumps({'counts':counts,'failures':failures,'ordered_ids_match':actual==expected}))
assert expected==[(test,n) for n in range(1,50)] and actual==expected
assert observed['namespaces']==[{'namespace':namespace,'status':'loaded'}]
assert not observed['test-failures'] and not observed['skips'] and not reference['skips']
assert not any(failures[k] for k in ['tests','namespaces','assertions','skip-mismatches'])
assert counts['pass']==49 and counts['guest-judged']==0
assert counts['fail']==counts['not-executed']==counts['skipped']==0
"#).arg(repository).arg(captured).output().unwrap();
    assert!(compared.status.success(), "{}\n{}", String::from_utf8_lossy(&compared.stdout),
        String::from_utf8_lossy(&compared.stderr));
    println!("{}", String::from_utf8_lossy(&compared.stdout));
}
