"""Audit newly executed failures separately from regressions of prior passes."""
from pathlib import Path
import collections,gzip,hashlib,json,re,subprocess,sys
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent/'suite-after-sort'
sys.path.insert(0,str(ROOT/'scripts'));import clojure_test_suite as suite
BASE='0436ab300980d8f883398585095aa2a9758151f0'
def original(path):return subprocess.check_output(['git','show',BASE+':'+path],cwd=ROOT)
reference_path='tests/oracle/clojure-test-suite-observations.json'
primary=(ROOT/reference_path).read_bytes();assert primary==original(reference_path)
# No profile, routing, count, skip or comparison change is made in this port.
for path in ['scripts/clojure_test_suite.py','tests/oracle/clojure-test-suite-suite.json']:
 if (ROOT/path).exists():assert (ROOT/path).read_bytes()==original(path)
path='crates/suss-compile/tests/clojure_test_suite.rs'
assert (ROOT/path).read_bytes().startswith(original(path))
base_capture='results/m12-suite-platform/suite-after-platform/suite-capture/suite-observations.json.gz'
previous=json.loads(gzip.decompress(original(base_capture)))
observed_bytes=(OUT/'suite-capture/suite-observations.json').read_bytes();observed=json.loads(observed_bytes);reference=json.loads(primary)
base_path='tests/oracle/clojure-test-suite-known-failures.json';old_bytes=original(base_path);old=json.loads(old_bytes)
old_counts,old_measured=suite.compare(reference,previous);assert old_measured==old
counts,new=suite.compare(reference,observed)
assert new['skip-mismatches']==old['skip-mismatches']
def identities(rows):return {f"{x['test']}#{x['ordinal']}":x for x in rows}
actual=identities(observed['assertions']);before=identities(previous['assertions']);primary_rows={x['id']:x for x in reference['assertions']}
changes={key:[{'id':k,'old':old[key].get(k),'new':new[key].get(k)} for k in sorted(set(old[key])|set(new[key])) if old[key].get(k)!=new[key].get(k)] for key in ['assertions','tests','namespaces']}
# Every prior independently comparable pass must remain present and pass.
regressions=[];prior_passes=0
for identity,row in primary_rows.items():
 if identity in before and suite.decide(row,before[identity])[0] is None:
  prior_passes+=1
  if identity not in actual or suite.decide(row,actual[identity])[0] is not None:
   regressions.append(identity)
assert not regressions,regressions
newly_executed_failures=[];other_new_failures=[]
for identity in new['assertions'].keys()-old['assertions'].keys():
 row=primary_rows[identity];namespace=row['test'].split('/')[0]
 item={'id':identity,'form':row['form'],'reason':new['assertions'][identity],'actual':actual.get(identity)}
 if identity not in before and (namespace in old['namespaces'] or row['test'] in old['tests']):newly_executed_failures.append(item)
 else:other_new_failures.append(item)
assert not other_new_failures,other_new_failures
for row in changes['assertions']:row.update(primary=primary_rows[row['id']],actual=actual.get(row['id']))
def decimal(x):
 if isinstance(x,dict):return x.get('tag')=='decimal' or any(decimal(v) for v in x.values())
 if isinstance(x,list):return any(decimal(v) for v in x)
 return False
decimal_rows=[x for x in reference['assertions'] if decimal(x)];assert len(decimal_rows)==200
decimal_outcomes=collections.Counter();decimal_missing=[]
for row in decimal_rows:
 if row['id'] not in actual:
  reason=new['assertions'].get(row['id']) or new['namespaces'].get(row['test'].split('/')[0]) or new['tests'].get(row['test']) or 'missing'
  decimal_outcomes['not-executed']+=1;decimal_missing.append({'id':row['id'],'reason':reason});continue
 reason,judged=suite.decide(row,actual[row['id']]);decimal_outcomes['fail' if reason else 'guest-judged' if judged else 'pass']+=1
assert sum(decimal_outcomes.values())==len(decimal_rows)
source_candidates=[]
for row in reference['assertions']:
 if re.search(r'(?:\d(?:\.\d*)?(?:[eE][+-]?\d+)?M\b|\b(?:bigdec|decimal\?|with-precision)\b)',row.get('form','')):
  reason=new['assertions'].get(row['id']) or new['namespaces'].get(row['test'].split('/')[0]) or new['tests'].get(row['test'])
  if reason:source_candidates.append({'id':row['id'],'form':row['form'],'reason':reason,'actual':actual.get(row['id'])})
report={'base':BASE,'capture_sha256':hashlib.sha256(observed_bytes).hexdigest(),'primary_sha256':hashlib.sha256(primary).hexdigest(),'raw_assertions':len(observed['assertions']),'old_counts':old_counts,'counts':counts,'changed_record_counts':{k:len(v) for k,v in changes.items()},'changes':changes,'prior_passes_preserved':prior_passes,'previously_passing_regressions':regressions,'newly_executed_failures':newly_executed_failures,'other_new_failures':other_new_failures,'primary_and_count_skip_guards_unchanged':True,'decimal_value_observations':dict(decimal_outcomes),'unresolved_decimal_source_candidates':len(source_candidates),'unresolved_candidates_by_namespace':dict(collections.Counter(x['id'].split('/')[0] for x in source_candidates))}
for name,value in [('suite-audit.json',report),('decimal-value-unexecuted.json',decimal_missing),('decimal-source-candidates.json',source_candidates),('proposed-known-failures.json',new)]:
 (OUT/name).write_text(json.dumps(value,indent=2,sort_keys=True)+'\n')
archive=OUT/'reviewed-before-known-failures.json.gz'
if not archive.exists():archive.write_bytes(gzip.compress(old_bytes,mtime=0))
assert gzip.decompress(archive.read_bytes())==old_bytes
# A baseline write is a separate reviewed step, never an automatic consequence.
print({k:v for k,v in report.items() if k not in ['changes','unresolved_candidates_by_namespace','newly_executed_failures']})
