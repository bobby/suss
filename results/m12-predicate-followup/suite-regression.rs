
/// Complete vendored bodies retain the two documented one-unit String/char
/// platform differences; every numeric-category assertion must match the JVM.
#[test]
fn pinned_core_predicate_bodies_match_primary_numeric_and_collection_values() {
    let repository = root();
    let workspace = tempfile::tempdir().unwrap();
    let source_root = workspace.path().join("src");
    let generated = Command::new("python3")
        .arg(repository.join("scripts/clojure_test_suite.py"))
        .args(["suss-sources", "--suite", "suite", "--output"])
        .arg(&source_root).output().unwrap();
    assert!(generated.status.success(), "{}", String::from_utf8_lossy(&generated.stderr));
    let stems = ["char", "coll", "sorted", "pos-int", "neg-int"];
    let names = stems.map(|name| format!("clojure.core-test.{name}-qmark"));
    let tests = stems.map(|name| format!("clojure.core-test.{name}-qmark/test-{name}?"));
    let observations = Harness::new(&source_root)
        .run(&json!({"namespaces": names, "tests": tests}), &source_root);
    let captured = workspace.path().join("predicate-observations.json");
    std::fs::write(&captured, serde_json::to_vec_pretty(&observations).unwrap()).unwrap();
    if let Some(directory) = std::env::var_os("SUSS_CORE_PREDICATE_CAPTURE") {
        let directory = PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::copy(&captured, directory.join("predicate-observations.json")).unwrap();
    }
    let compared = Command::new("python3").arg("-c").arg(r#"
import json,sys
from pathlib import Path
root=Path(sys.argv[1]);sys.path.insert(0,str(root/'scripts'))
import clojure_test_suite as suite
reference=json.loads((root/'tests/oracle/clojure-test-suite-observations.json').read_text())
stems=['char','coll','sorted','pos-int','neg-int']
names=['clojure.core-test.'+n+'-qmark' for n in stems]
tests=['clojure.core-test.'+n+'-qmark/test-'+n+'?' for n in stems]
reference.update(namespaces=names,tests=tests,
 assertions=[x for x in reference['assertions'] if x['test'] in tests],
 skips=[x for x in reference['skips'] if x['namespace'] in names])
observed=json.loads(Path(sys.argv[2]).read_text())
counts,failures=suite.compare(reference,observed)
expected=sorted((x['test'],x['ordinal']) for x in reference['assertions'])
actual=sorted((x['test'],x['ordinal']) for x in observed['assertions'])
print(json.dumps({'counts':counts,'failures':failures,'expected_count':len(expected),
 'actual_count':len(actual),'all_ids_match':actual==expected}))
assert len(expected)==173 and actual==expected
assert not any(failures[k] for k in ['tests','namespaces','skip-mismatches'])
# Accepted design: a one-unit UTF-16 string is also a character. Do not count
# these primary JVM String judgments as matching, skip them, or forge a JVM type.
assert failures['assertions']=={
 'clojure.core-test.char-qmark/test-char?#33':'verdict:fail',
 'clojure.core-test.char-qmark/test-char?#34':'verdict:fail'}
assert counts['pass']==171 and counts['guest-judged']==0 and counts['fail']==2
assert counts['not-executed']==0 and counts['skipped']==0
"#).arg(&repository).arg(&captured).output().unwrap();
    assert!(compared.status.success(), "{}\n{}", String::from_utf8_lossy(&compared.stdout),
        String::from_utf8_lossy(&compared.stderr));
    println!("PINNED_CORE_PREDICATE_FIELDS {}", String::from_utf8_lossy(&compared.stdout));
}
