"""Require every original char assertion, independently of baseline drift."""
from pathlib import Path
import hashlib,json,subprocess,sys
ROOT=Path(__file__).resolve().parents[2];OUT=ROOT/'results/m12-reader-followup'/sys.argv[1]
sys.path.insert(0,str(ROOT/'scripts'));import clojure_test_suite as suite
primary_path=ROOT/'tests/oracle/clojure-test-suite-observations.json';primary=primary_path.read_bytes()
assert hashlib.sha256(primary).hexdigest()=='449264ab6bca1dcf761d0204e4a64de4de91562e576615c4e84a63246839f4c7'
source_path='vendor/clojure-test-suite/upstream/test/clojure/core_test/char.cljc'
source=(ROOT/source_path).read_bytes();assert source==subprocess.check_output(['git','show','a783ad60540b4bc22820e8811a568132a0b14210:'+source_path],cwd=ROOT)
reference=json.loads(primary);raw=(OUT/'suite-capture/suite-observations.json').read_bytes();observed=json.loads(raw)
namespace='clojure.core-test.char';test=namespace+'/test-char'
expected=[r for r in reference['assertions'] if r['test']==test];actual=[r for r in observed['assertions'] if r['test']==test]
assert len(expected)==9
state=[n for n in observed['namespaces'] if n['namespace']==namespace]
assert len(state)==1 and state[0]['status']=='loaded',state
assert [(r['test'],r['ordinal']) for r in actual]==[(r['test'],r['ordinal']) for r in expected], 'complete original ordered assertion body required'
assert not any(r.get('test')==test for r in observed['test-failures'])
assert not any(r.get('namespace')==namespace for r in observed['skips'])
counts,failures=suite.compare(reference,observed)
assert namespace not in failures['namespaces'] and test not in failures['tests']
assert not any(r['id'] in failures['assertions'] for r in expected)
verdicts=[suite.decide(e,a) for e,a in zip(expected,actual)]
assert all(reason is None for reason,_ in verdicts),verdicts
report={'char_assertions':9,'host_matches':sum(not guest for _,guest in verdicts),'guest_judgments':sum(guest for _,guest in verdicts),'missing':0,'ordered_ids_match':True,'primary_sha256':hashlib.sha256(primary).hexdigest(),'source_sha256':hashlib.sha256(source).hexdigest(),'capture_sha256':hashlib.sha256(raw).hexdigest(),'full_suite_counts':counts,'qualification':'This proves the complete original char body. It does not certify broader EDN/numeric compatibility or relabel the full native comparison result.'}
p=OUT/'char-body-audit.json';assert not p.exists();p.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
