from pathlib import Path
import collections,copy,gzip,hashlib,json,subprocess,sys
E=Path(__file__).resolve().parent;C=E.parents[2] if E.name=='baseline-before-review' else E.parent/'native-host-var-foundation';J=Path('/home/bobby/.codex-workspaces/worktrees/ios-cd7ccbc9-1c87-43fe-95e7-2b6b71168849/suss/target/probe/m12-case-budget-source');out=C/'results/native-host-foundation/baseline-before-review';receipt=json.loads((out/'commands.json').read_text());assert receipt['complete'] and receipt['inputs_unchanged'] and receipt['binaryen_unchanged'];assert receipt['commands'][0]['exit_code']==101
sys.path.insert(0,str(C/'scripts'));import clojure_test_suite as suite
sha=lambda b:hashlib.sha256(b).hexdigest();reference_raw=(C/'tests/oracle/clojure-test-suite-observations.json').read_bytes();assert sha(reference_raw)=='449264ab6bca1dcf761d0204e4a64de4de91562e576615c4e84a63246839f4c7'
for name in ['scripts/clojure_test_suite.py','tests/oracle/clojure-test-suite-observations.json','crates/suss-compile/tests/clojure_test_suite.rs']:
 assert sha((C/name).read_bytes())==receipt['inputs'][name],name
baseline_raw=gzip.decompress((out/'old-baseline.json.gz').read_bytes()) if (out/'old-baseline.json.gz').exists() else (C/'tests/oracle/clojure-test-suite-known-failures.json').read_bytes();assert sha(baseline_raw)=='6fedd3e20fe35172c12e45c7760d6ce6e1859980e15ad3c6e2f25f4646f1992e'
reference=json.loads(reference_raw);baseline=json.loads(baseline_raw);actual_raw=(out/'capture/suite-observations.json').read_bytes();actual=json.loads(actual_raw);assert [r['namespace'] for r in actual['namespaces']]==reference['namespaces'];counts,failures=suite.compare(reference,actual);expected=copy.deepcopy(baseline)
for ns in ['clojure.core-test.symbol','clojure.core-test.var-qmark']:
 entry=expected['namespaces'].pop(ns);assert entry['stage']=='namespace' and 'Unresolved Runtime name var at bytes' in entry['diagnostic']
for ns,before,after in [('clojure.core-test.add-watch','alter-var-root','ref'),('clojure.core-test.remove-watch','alter-var-root','ref'),('clojure.core-test.ifn-qmark','var','promise')]:
 e=expected['namespaces'][ns];needle='Unresolved Runtime name '+before+' at bytes';assert e['stage']=='namespace' and e['diagnostic'].count(needle)==1;e['diagnostic']=e['diagnostic'].replace(needle,'Unresolved Runtime name '+after+' at bytes')
assert failures==expected,'Unreviewed failure/classification change'
assert counts=={'fail':105,'guest-judged':101,'namespaces-failed':64,'not-executed':1408,'pass':5325,'skipped':0},counts
previous_raw=gzip.decompress(((out/'previous-full-observations.json.gz') if (out/'previous-full-observations.json.gz').exists() else (J/'results/m12-eval/root-diagnostic-native-checkpoint/complete-capture/suite-capture/suite-observations.json.gz')).read_bytes());assert sha(previous_raw)=='9fd9c7c33212d2cea78319932ca49622be569c162ff54c92a07b0a5551c1404f';previous=json.loads(previous_raw);_,old_full_failures=suite.compare(reference,previous);expected_full=copy.deepcopy(expected);expected_full['namespaces'].pop('clojure.core-test.eval');assert old_full_failures==expected_full
key=lambda r:r['test']+'#'+str(r['ordinal']);refs={r['id']:r for r in reference['assertions']};rows={key(r):r for r in actual['assertions']};assert len(rows)==len(actual['assertions']);prior=[r for r in previous['assertions'] if not r['test'].startswith('clojure.core-test.eval/')];assert [key(r) for r in prior]==[key(r) for r in actual['assertions']]
for old in prior:assert suite.decide(refs[key(old)],old)==suite.decide(refs[key(old)],rows[key(old)]),key(old)
bodies={}
for ns,leaf,total,guests in [('clojure.core-test.symbol','test-symbol',61,[]),('clojure.core-test.var-qmark','test-var?',34,list(range(1,9)))]:
 body=[r for r in actual['assertions'] if r['test']==ns+'/'+leaf];assert [r['ordinal'] for r in body]==list(range(1,total+1));seen=[]
 for row in body:
  reason,guest=suite.decide(refs[key(row)],row);assert reason is None,(key(row),reason)
  if guest:seen.append(row['ordinal'])
 assert seen==guests;bodies[ns]={'rows':total,'host_matches':total-len(guests),'guest_ordinals':guests}
assert not any(r['test'].startswith('clojure.core-test.eval/') for r in actual['assertions'])
report={'qualification':'Review of actual native101 before ledger correction. Only two completely executed namespaces are removed and three observed diagnostics advance. Public eval remains absent on the core foundation. No numeric or full workspace completion claim.','capture_sha256':sha(actual_raw),'old_full_capture_sha256':sha(previous_raw),'primary_sha256':sha(reference_raw),'baseline_before_sha256':sha(baseline_raw),'counts':counts,'raw_rows':len(rows),'all_non_eval_prior_decisions_preserved':len(prior),'newly_loaded_bodies':bodies,'namespace_only_changes':5,'actual_exit_before':101}
(out/'review.json').write_text(json.dumps(report,indent=2)+'\n');(out/'proposed-known-failures.json').write_text(json.dumps(expected,indent=1)+'\n');(out/'old-baseline.json.gz').write_bytes(gzip.compress(baseline_raw,mtime=0));(out/'previous-full-observations.json.gz').write_bytes(gzip.compress(previous_raw,mtime=0));print(report)
