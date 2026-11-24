"""Review complete eval execution without weakening the primary or old guards."""
from pathlib import Path
import collections,copy,gzip,hashlib,json,subprocess,sys
ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'scripts'));import clojure_test_suite as suite
PARENT='974ed7e7669545772a624a5c9ba11c5098822c97'
PRIMARY_SHA='449264ab6bca1dcf761d0204e4a64de4de91562e576615c4e84a63246839f4c7'
BASELINE_SHA='6fedd3e20fe35172c12e45c7760d6ce6e1859980e15ad3c6e2f25f4646f1992e'
PREVIOUS_SHA='926f27f1becc38f75bba59956eefbe53a9d99b0a9a15f0886620a228e7639971'
sha=lambda raw:hashlib.sha256(raw).hexdigest()
def parent(path):return subprocess.check_output(['git','show',PARENT+':'+path],cwd=ROOT)
def review(primary_raw,baseline_raw,previous_raw,observed_raw):
 assert sha(primary_raw)==PRIMARY_SHA and sha(baseline_raw)==BASELINE_SHA and sha(previous_raw)==PREVIOUS_SHA
 reference,baseline,previous,observed=map(json.loads,[primary_raw,baseline_raw,previous_raw,observed_raw])
 before_counts,before_failures=suite.compare(reference,previous);assert before_failures==baseline
 counts,measured=suite.compare(reference,observed)
 expected=copy.deepcopy(baseline);assert expected['namespaces'].pop('clojure.core-test.eval')['stage']=='namespace'
 assert measured==expected,'unexpected failure change; inspect the complete proposed baseline'
 assert [r['namespace'] for r in observed['namespaces']]==reference['namespaces']
 refs={row['id']:row for row in reference['assertions']}
 actual={row['test']+'#'+str(row['ordinal']):row for row in observed['assertions']}
 assert len(actual)==len(observed['assertions'])
 prior=0
 for row in previous['assertions']:
  key=row['test']+'#'+str(row['ordinal'])
  if suite.decide(refs[key],row)[0] is None:
   assert key in actual and suite.decide(refs[key],actual[key])[0] is None,key
   prior+=1
 test='clojure.core-test.eval/test-eval'
 body=[row for row in observed['assertions'] if row['test']==test]
 assert [row['ordinal'] for row in body]==list(range(1,35))
 assert next(row for row in observed['namespaces'] if row['namespace']=='clojure.core-test.eval')=={'namespace':'clojure.core-test.eval','status':'loaded'}
 guests=[]
 for row in body:
  key=row['test']+'#'+str(row['ordinal']);reason,guest=suite.decide(refs[key],row);assert reason is None,(key,reason)
  if guest:guests.append(row['ordinal'])
 assert guests==[15,16,17,18,19]
 for ordinal in [5,6,7]:assert suite.decide(refs[test+'#'+str(ordinal)],body[ordinal-1])==(None,False)
 wanted=before_counts.copy()
 for key,change in [('pass',29),('guest-judged',5),('not-executed',-34),('namespaces-failed',-1)]:wanted[key]+=change
 assert counts==wanted,'primary/count/skip/guest rules changed'
 assert len(observed['assertions'])==len(previous['assertions'])+34
 def tagged(node,tags):
  if isinstance(node,dict):return node.get('tag') in tags or any(tagged(v,tags) for v in node.values())
  if isinstance(node,list):return any(tagged(v,tags) for v in node)
  return False
 numeric={}
 for name,tags,total in [('bigint-ratio',{'bigint','biginteger','ratio'},229),('decimal',{'decimal'},200)]:
  selected=[row for row in reference['assertions'] if tagged(row,tags)];assert len(selected)==total
  for row in selected:
   assert row['id'] in actual and suite.decide(row,actual[row['id']])==(None,False),row['id']
  numeric[name]={'primary_rows':total,'strict_host_matches':total}
 return {'counts':counts,'raw_rows':len(actual),'prior_comparable_successes_preserved':prior,'eval':{'actual_rows':34,'host_matches':29,'guest_ordinals':guests},'numeric_subsets':numeric,'proposed_baseline':expected}

def main():
 out=Path(sys.argv[1]).resolve();receipt=json.loads((out/'suite-commands.json').read_text())
 assert receipt['complete'] and receipt['inputs_unchanged']
 assert all((ROOT/p).is_file() and sha((ROOT/p).read_bytes())==digest for p,digest in receipt['inputs'].items())
 commands={entry['label']:entry for entry in receipt['commands']}
 assert commands['suite-build']['exit_code']==commands['suite-lock']['exit_code']==0
 assert commands['suite-run']['exit_code']==101,'retain the original native discrepancy'
 for path in ['scripts/clojure_test_suite.py','tests/oracle/clojure-test-suite-observations.json']:
  assert (ROOT/path).read_bytes()==parent(path),path
 old=parent('crates/suss-compile/tests/clojure_test_suite.rs').decode()
 expected=old.replace('mod pinned_some_fn_bodies;\n','mod pinned_some_fn_bodies;\n#[path = "support/pinned_eval_body.rs"]\nmod pinned_eval_body;\n',1)
 assert (ROOT/'crates/suss-compile/tests/clojure_test_suite.rs').read_text()==expected,'full harness changed'
 raw=(out/'suite-capture/suite-observations.json').read_bytes()
 previous=gzip.decompress((ROOT/'results/m12-reader-followup/full-suite-after-random-and-assert/suite-capture/suite-observations.json.gz').read_bytes())
 baseline=(ROOT/'tests/oracle/clojure-test-suite-known-failures.json').read_bytes()
 report=review((ROOT/'tests/oracle/clojure-test-suite-observations.json').read_bytes(),baseline,previous,raw)
 proposed=report.pop('proposed_baseline')
 report.update(primary_sha256=PRIMARY_SHA,parent_baseline_sha256=BASELINE_SHA,parent_capture_sha256=PREVIOUS_SHA,capture_sha256=sha(raw),original_native_exit_code=101,baseline_written=False,qualification='Host review of an observed native101. Only the completed eval namespace failure is removed; native rerun/workspace acceptance remain separate.')
 for name,value in [('complete-capture-review.json',report),('proposed-known-failures.json',proposed)]:
  path=out/name;assert not path.exists();path.write_text(json.dumps(value,indent=2,sort_keys=True)+'\n')
 (out/'parent-random-baseline.json.gz').write_bytes(gzip.compress(baseline,mtime=0))
 print(json.dumps(report,indent=2))
if __name__=='__main__':main()
