//! Complete numeric-dependent source bodies; preserve all primary identities,
//! existing opaque rules and accepted narrow-value platform differences.
use super::*;

fn capture_numeric_body(stems: &[&str], leaves: &[&str], output_name: &str) -> (tempfile::TempDir, PathBuf, PathBuf) {
    let repository = root();
    let workspace = tempfile::tempdir().unwrap();
    let source_root = workspace.path().join("src");
    let generated = Command::new("python3").arg(repository.join("scripts/clojure_test_suite.py"))
        .args(["suss-sources", "--suite", "suite", "--output"]).arg(&source_root).output().unwrap();
    assert!(generated.status.success(), "{}", String::from_utf8_lossy(&generated.stderr));
    let names = stems.iter().map(|stem| format!("clojure.core-test.{stem}")).collect::<Vec<_>>();
    let tests = names.iter().zip(leaves).map(|(name, leaf)| format!("{name}/{leaf}")).collect::<Vec<_>>();
    let observed = Harness::new(&source_root).run(&json!({"namespaces":names,"tests":tests}), &source_root);
    let captured = workspace.path().join(output_name);
    std::fs::write(&captured, serde_json::to_vec_pretty(&observed).unwrap()).unwrap();
    if let Some(directory) = std::env::var_os("SUSS_NUMERIC_BODY_CAPTURE") {
        let directory=PathBuf::from(directory);std::fs::create_dir_all(&directory).unwrap();
        std::fs::copy(&captured,directory.join(output_name)).unwrap();
    }
    (workspace,repository,captured)
}

#[test]
fn complete_eq_and_not_eq_bodies_match_all_original_primary_assertions() {
    let (_workspace,repository,captured)=capture_numeric_body(&["eq","not-eq"], &["test-eq","test-not-eq"], "equality-observations.json");
    let compared=Command::new("python3").arg("-c").arg(r#"
import hashlib,json,sys
from pathlib import Path
root=Path(sys.argv[1]);sys.path.insert(0,str(root/'scripts'));import clojure_test_suite as suite
raw=(root/'tests/oracle/clojure-test-suite-observations.json').read_bytes()
assert hashlib.sha256(raw).hexdigest()=='449264ab6bca1dcf761d0204e4a64de4de91562e576615c4e84a63246839f4c7'
reference=json.loads(raw);names=['clojure.core-test.eq','clojure.core-test.not-eq'];tests=[names[0]+'/test-eq',names[1]+'/test-not-eq']
reference.update(namespaces=names,tests=tests,assertions=[r for r in reference['assertions'] if r['test'] in tests],skips=[r for r in reference['skips'] if r['namespace'] in names])
observed=json.loads(Path(sys.argv[2]).read_text());counts,failures=suite.compare(reference,observed)
expected=[(r['test'],r['ordinal']) for r in reference['assertions']];actual=[(r['test'],r['ordinal']) for r in observed['assertions']]
assert expected==actual==[(t,n) for t in tests for n in range(1,72)],(len(expected),len(actual),failures)
assert observed['namespaces']==[{'namespace':n,'status':'loaded'} for n in names]
assert not observed['test-failures'] and not observed['skips'] and not reference['skips']
assert not any(failures[k] for k in ['tests','namespaces','assertions','skip-mismatches']),failures
assert counts['pass']==142 and counts['guest-judged']==0
assert counts['fail']==counts['not-executed']==counts['skipped']==0
for t in tests:
 for ordinal in [12,68,69]:
  wanted=next(r for r in reference['assertions'] if (r['test'],r['ordinal'])==(t,ordinal));got=next(r for r in observed['assertions'] if (r['test'],r['ordinal'])==(t,ordinal));assert suite.decide(wanted,got)==(None,False)
print(json.dumps({'counts':counts,'ordered_ids_match':True}))
"#).arg(repository).arg(captured).output().unwrap();
    assert!(compared.status.success(),"{}\n{}",String::from_utf8_lossy(&compared.stdout),String::from_utf8_lossy(&compared.stderr));
    println!("{}",String::from_utf8_lossy(&compared.stdout));
}

#[test]
fn complete_default_num_body_retains_numeric_rows_and_explicit_narrow_platform_differences() {
    let (_workspace,repository,captured)=capture_numeric_body(&["num"], &["test-num"], "num-observations.json");
    let compared=Command::new("python3").arg("-c").arg(r#"
import hashlib,json,sys
from pathlib import Path
root=Path(sys.argv[1]);sys.path.insert(0,str(root/'scripts'));import clojure_test_suite as suite
primary=root/'results/m12-numeric-source-bodies/num-selected-primary'
receipt=json.loads((primary/'command.json').read_text())
assert receipt['exit_code']==1 and receipt['inputs_unchanged']
reference=json.loads((primary/'reference.json').read_text())
reference_build=json.loads((primary/'reference-build.json').read_text())
assert reference_build['runs']==4 and reference_build['primary_exit_codes']==[1]*4
assert reference_build['preserved_primary_failures']==[7,8,9]
assert hashlib.sha256((primary/'reference.json').read_bytes()).hexdigest()==reference_build['output_sha256']
assert reference['schema']==2 and reference['upstream']=='clojure-1.12.6' and reference['feature-selection']==['suss','default']
assert reference['summary']=={'test':1,'pass':23,'fail':3,'error':0} and not reference['reporter-errors']
# This is the complete original source under Suss/default reader selection,
# not a replacement for the original JVM/clj 18-row primary or its comparator.
original=root/'vendor/clojure-test-suite/upstream/test/clojure/core_test/num.cljc'
assert hashlib.sha256(original.read_bytes()).hexdigest()==next(v for p,v in receipt['inputs'].items() if p.endswith('/clojure/core_test/num.cljc'))
observed=json.loads(Path(sys.argv[2]).read_text());counts,failures=suite.compare(reference,observed)
test='clojure.core-test.num/test-num';expected=[(test,n) for n in range(1,27)]
assert [(r['test'],r['ordinal']) for r in reference['assertions']]==[(r['test'],r['ordinal']) for r in observed['assertions']]==expected
assert observed['namespaces']==[{'namespace':'clojure.core-test.num','status':'loaded'}]
assert not observed['test-failures'] and not observed['skips'] and not reference['skips']
assert not any(failures[k] for k in ['tests','namespaces','skip-mismatches']),failures
# ADR-0003 rejects narrow boxed values. The actual JVM default-branch checks
# for preserving Short/Byte/Integer class fail, while Suss's Long inputs pass.
# Keep these three discrepancies explicit; never call them host matches.
platform={7:'short',8:'byte',9:'int'}
assert set(failures['assertions'])=={test+'#'+str(n) for n in platform},failures
for ordinal,kind in platform.items():
 wanted=reference['assertions'][ordinal-1];got=observed['assertions'][ordinal-1]
 assert wanted['kind']==got['kind']=='value' and wanted['verdict']=='fail' and got['verdict']=='pass'
 assert wanted['value']=={'tag':'bool','value':False} and got['value']=={'tag':'bool','value':True}
 assert '('+kind+' 1)' in wanted['form']
assert counts['pass']==23 and counts['fail']==3 and counts['guest-judged']==0
assert counts['not-executed']==counts['skipped']==0
for ordinal in [4,5,6]:
 assert suite.decide(reference['assertions'][ordinal-1],observed['assertions'][ordinal-1])==(None,False)
print(json.dumps({'counts':counts,'explicit_narrow_platform_ordinals':list(platform),'ordered_ids_match':True}))
"#).arg(repository).arg(captured).output().unwrap();
    assert!(compared.status.success(),"{}\n{}",String::from_utf8_lossy(&compared.stdout),String::from_utf8_lossy(&compared.stderr));
    println!("{}",String::from_utf8_lossy(&compared.stdout));
}
