"""Write only the independently audited outcome changes, retaining failures."""
from pathlib import Path
import hashlib,json,subprocess
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent/'suite-after-sort'
report=json.loads((OUT/'suite-audit.json').read_text());proposed=json.loads((OUT/'proposed-known-failures.json').read_text())
expected_new={**{f'clojure.core-test.assoc/test-assoc#{n}':'verdict:fail' for n in [31,32,34,35]},**{f'clojure.core-test.char-qmark/test-char?#{n}':'verdict:fail' for n in [33,34]},'clojure.core-test.dissoc/test-dissoc#17':'observation:undecodable'}
assert {x['id']:x['reason'] for x in report['newly_executed_failures']}==expected_new
resolved={'clojure.core-test.atom/test-atom#6','clojure.core-test.counted-qmark/test-counted?#6','clojure.core-test.counted-qmark/test-counted?#7','clojure.core-test.interpose/test-interpose#4','clojure.core-test.seq/test-seq#11'}
assert {x['id'] for x in report['changes']['assertions'] if x['new'] is None}==resolved
assert len(report['changes']['assertions'])==12 and not report['changes']['tests']
assert len([x for x in report['changes']['namespaces'] if x['new'] is None])==10
changed=[x for x in report['changes']['namespaces'] if x['new'] is not None]
assert len(changed)==1 and changed[0]['id']=='clojure.core-test.sort-by' and 'Unresolved Runtime name sort-by' in changed[0]['new']['diagnostic']
assert report['primary_and_count_skip_guards_unchanged'] and not report['previously_passing_regressions'] and not report['other_new_failures']
assert report['counts']=={'pass':4770,'guest-judged':75,'fail':86,'not-executed':2008,'skipped':0,'namespaces-failed':73}
assert report['decimal_value_observations']=={'pass':194,'not-executed':6} and report['unresolved_decimal_source_candidates']==15
path='tests/oracle/clojure-test-suite-known-failures.json';before=(ROOT/path).read_bytes();original=subprocess.check_output(['git','show',report['base']+':'+path],cwd=ROOT);assert before==original
command=['python3','scripts/clojure_test_suite.py','compare','--observations',str(OUT/'suite-capture/suite-observations.json'),'--write']
record={'review':'Exact7 newly executed failures retained,5 unknown sorted operands now independently decode,10 namespaces load and1 advances to missing sort-by. All previous passes and primary/count/skip/comparison decisions unchanged.','old_sha256':hashlib.sha256(before).hexdigest(),'commands':[]}
for name,args in [('reviewed-write',command),('reviewed-compare',command[:-1])]:
 r=subprocess.run(args,cwd=ROOT,text=True,capture_output=True);(OUT/(name+'.log')).write_text(r.stdout+r.stderr);record['commands'].append({'command':args,'exit_code':r.returncode});assert r.returncode==0,r.stdout+r.stderr
assert json.loads((ROOT/path).read_bytes())==proposed
record['new_sha256']=hashlib.sha256((ROOT/path).read_bytes()).hexdigest();(OUT/'reviewed-baseline-commands.json').write_text(json.dumps(record,indent=2)+'\n');print(record)
