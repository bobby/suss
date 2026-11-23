"""Audit newly executed EDN values without rewriting any primary or baseline."""
from pathlib import Path
import collections,hashlib,json,subprocess,sys
ROOT=Path(__file__).resolve().parents[2];OUT=ROOT/'results/m12-reader-followup'/sys.argv[1]
sys.path.insert(0,str(ROOT/'scripts'));import clojure_test_suite as suite
primary=ROOT/'tests/oracle/clojure-test-suite-observations.json'
raw=primary.read_bytes();assert hashlib.sha256(raw).hexdigest()=='449264ab6bca1dcf761d0204e4a64de4de91562e576615c4e84a63246839f4c7'
reference=json.loads(raw);observed_raw=(OUT/'suite-capture/suite-observations.json').read_bytes();observed=json.loads(observed_raw)
receipt=json.loads((OUT/'suite-commands.json').read_text());assert receipt['complete'] and receipt['inputs_unchanged']
assert [x['exit_code'] for x in receipt['commands']]==[0,101,0]
counts,failures=suite.compare(reference,observed)
assert counts=={'pass':5123,'guest-judged':81,'fail':105,'not-executed':1630,'skipped':0,'namespaces-failed':70},counts
namespace='clojure.edn-test.read-string';test=namespace+'/test-read-string'
expected=[x for x in reference['assertions'] if x['test'].split('/')[0]==namespace]
actual=[x for x in observed['assertions'] if x['test'].split('/')[0]==namespace]
assert len(expected)==len(actual)==297
assert [(x['test'],x['ordinal']) for x in actual]==[(x['test'],x['ordinal']) for x in expected]
assert {'namespace':namespace,'status':'loaded'} in observed['namespaces']
assert namespace not in failures['namespaces'] and test not in failures['tests']
assert not any(x['test'].split('/')[0]==namespace for x in observed['test-failures'])
assert not any(x['namespace']==namespace for x in observed['skips'])
error_ordinals=[95,96,188,*range(202,214),266,267]
wanted={test+'#'+str(n):'kind:error' for n in error_ordinals}
wanted.update({test+'#69':'observation:value',test+'#249':'verdict:fail'})
actual_failures={k:v for k,v in failures['assertions'].items() if k.startswith(namespace+'/')}
assert actual_failures==wanted,actual_failures
reasons=collections.Counter();details=[]
for exp,got in zip(expected,actual):
 reason,guest=suite.decide(exp,got);reasons[reason or ('guest-judged' if guest else 'host-match')]+=1
 if reason:
  item={'id':exp['id'],'form':exp['form'],'reason':reason,'source-head':got['source-head'],'head':got['head']}
  if got['kind']=='error':item['message']=''.join(chr(x) for x in got['thrown']['message']['units'])
  details.append(item)
assert reasons=={'host-match':272,'guest-judged':6,'kind:error':17,'observation:value':1,'verdict:fail':1},reasons
audit=json.loads((OUT/'suite-audit.json').read_text())
assert audit['prior_strict_passes_preserved']==4917 and audit['prior_individually_comparable_passes_preserved']==4932
assert not audit['previously_passing_regressions'] and not audit['raw_comparison_regressions']
assert audit['numeric_subsets']['decimal']['outcomes']=={'pass':199,'not-executed':1}
assert audit['numeric_subsets']['bigint-ratio']['outcomes']=={'pass':219,'not-executed':10}
report={'capture_sha256':hashlib.sha256(observed_raw).hexdigest(),'primary_sha256':hashlib.sha256(raw).hexdigest(),'edn_assertions':297,'ordered_ids_match':True,'outcomes':dict(reasons),'explicit_remaining_failures':details,'counts':counts,'baseline_written':False,'qualification':'New native execution with unchanged primary/comparator/body/count/skip rules; original Rust baseline-drift exit101 retained. Unsupported or incorrect reader values remain failures.'}
path=OUT/'edn-body-audit.json';assert not path.exists();path.write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({k:v for k,v in report.items() if k!='explicit_remaining_failures'},indent=2))
