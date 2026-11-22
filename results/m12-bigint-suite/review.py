"""Lossless per-assertion audit before accepting changed known failures."""
from pathlib import Path
import collections,gzip,hashlib,json,sys
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
sys.path.insert(0,str(ROOT/'scripts'));import clojure_test_suite as suite
ref_path=ROOT/'tests/oracle/clojure-test-suite-observations.json'
ref=json.loads(ref_path.read_text());obs_path=OUT/'suite-observations.json'
obs=json.loads(obs_path.read_text() if obs_path.exists() else gzip.decompress((OUT/'suite-observations.json.gz').read_bytes()))
before_path=OUT/'before-baseline.json'
if not before_path.exists():before_path.write_bytes((ROOT/'tests/oracle/clojure-test-suite-known-failures.json').read_bytes())
before=json.loads(before_path.read_text());counts,after=suite.compare(ref,obs)
for key in ['namespaces','tests','skip-mismatches']:assert before[key]==after[key],key
refs={r['id']:r for r in ref['assertions']};actuals={r['test']+'#'+str(r['ordinal']):r for r in obs['assertions']}
remaining={f'clojure.core-test.slash/test-slash#{n}' for n in [*range(5,17),83,84,85]}
rows=[]
for identity in sorted(set(before['assertions'])|set(after['assertions'])):
 old,new=before['assertions'].get(identity),after['assertions'].get(identity)
 if old==new:continue
 assert old=='kind:error',(identity,old,new)
 expected,actual=refs[identity],actuals[identity]
 if new is None:
  assert suite.decide(expected,actual)==(None,False),(identity,suite.decide(expected,actual))
  reason='Host independently matches the full primary observation and verdict; no guest-only or opaque success.'
 else:
  assert identity in remaining and new=='verdict:fail',(identity,new)
  assert expected['operands'][1]['tag']=='bigint' and actual['operands'][1]['tag']=='f64',identity
  reason='Known exact-division gap: BigInt operands execute but the result is still double. Preserve the failing verdict; Ratio/exact division remains required.'
 rows.append({'id':identity,'before':old,'after':new,'rationale':reason,'primary':expected,'observed':actual})
assert len(rows)==88 and sum(row['after'] is None for row in rows)==73
assert counts['guest-judged']==54 and counts['not-executed']==4143 and counts['namespaces-failed']==124
record={'reference_sha256':hashlib.sha256(ref_path.read_bytes()).hexdigest(),'counts':counts,'changes':rows,
        'unchanged_namespaces_tests_and_skips':True,'new_opaque_or_guest_only_passes':0}
(OUT/'review.json').write_text(json.dumps(record,indent=2)+'\n')
(OUT/'candidate-baseline.json').write_text(json.dumps(after,indent=1)+'\n')
if '--write' in sys.argv:(ROOT/'tests/oracle/clojure-test-suite-known-failures.json').write_text(json.dumps(after,indent=1)+'\n')
print(counts);print(len(rows),'reviewed changes;',73,'independently verified new passes;',15,'remaining exact-division failures')
