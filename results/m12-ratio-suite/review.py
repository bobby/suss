"""Audit every changed assertion and newly enabled namespace against the primary."""
from pathlib import Path
import gzip,hashlib,json,sys
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
sys.path.insert(0,str(ROOT/'scripts'));import clojure_test_suite as suite
ref_path=ROOT/'tests/oracle/clojure-test-suite-observations.json';ref=json.loads(ref_path.read_text())
path=OUT/'suite-observations.json';obs=json.loads(path.read_text() if path.exists() else gzip.decompress((OUT/'suite-observations.json.gz').read_bytes()))
previous=OUT/'before-baseline.json'
if not previous.exists():previous.write_bytes((ROOT/'tests/oracle/clojure-test-suite-known-failures.json').read_bytes())
before=json.loads(previous.read_text());counts,after=suite.compare(ref,obs)
enabled={'clojure.core-test.denominator','clojure.core-test.numerator','clojure.core-test.rationalize'}
assert set(before['namespaces'])-set(after['namespaces'])==enabled
assert set(after['namespaces'])-set(before['namespaces'])==set()
for key in set(after['namespaces']):assert before['namespaces'][key]==after['namespaces'][key]
for key in ['tests','skip-mismatches']:assert before[key]==after[key]
refs={r['id']:r for r in ref['assertions']};actual={r['test']+'#'+str(r['ordinal']):r for r in obs['assertions']}
new_failures={f'clojure.core-test.rationalize/test-rationalize#{n}' for n in [10,11,15,16]}
rows=[]
for identity in sorted(set(before['assertions'])|set(after['assertions'])):
 old,new=before['assertions'].get(identity),after['assertions'].get(identity)
 if old==new:continue
 expected,found=refs[identity],actual[identity]
 if new is None:
  assert old in ['kind:error','verdict:fail'] and suite.decide(expected,found)==(None,False),(identity,old,new)
  reason='The complete host observation and primary verdict match; no opaque or guest-only result is promoted.'
 else:
  assert identity in new_failures and old is None and new=='kind:error'
  assert 'M)' in expected['form'] and expected['test'].rsplit('/',1)[0] in enabled
  assert found['kind']=='error'
  reason='Previously unexecuted namespace now reaches this assertion, but its BigDecimal literal is still unsupported. Preserve the failure for #339.'
 rows.append({'id':identity,'before':old,'after':new,'rationale':reason,'primary':expected,'observed':found})
assert len(rows)==98 and sum(r['after'] is None for r in rows)==94
new_rows=[]
for expected in ref['assertions']:
 namespace=expected['test'].rsplit('/',1)[0]
 if namespace not in enabled:continue
 found=actual[expected['id']];reason,judged=suite.decide(expected,found)
 assert not judged
 assert reason is None or expected['id'] in new_failures
 note=''
 if expected['kind']=='thrown' and 'M)' in expected['form']:
  note='A thrown assertion can pass while evaluating an unsupported Decimal argument; this is not proof of Decimal or callee behavior.'
 new_rows.append({'primary':expected,'observed':found,'failure':reason,'limitation':note})
assert len(new_rows)==36 and sum(r['failure'] is None for r in new_rows)==32
assert counts=={'pass':2512,'guest-judged':54,'fail':266,'not-executed':4107,'skipped':0,'namespaces-failed':121}
record={'reference_sha256':hashlib.sha256(ref_path.read_bytes()).hexdigest(),'counts':counts,'changes':rows,'new_namespace_assertions':new_rows,'new_opaque_or_guest_only_passes':0,'unchanged_tests_and_skips':True,'limitations':'The four Decimal rationalize failures remain. Some thrown assertions do not prove callee execution when Decimal argument evaluation itself is unsupported. This audit is a baseline review, not numeric-tower acceptance.'}
(OUT/'review.json').write_text(json.dumps(record,indent=2)+'\n');(OUT/'candidate-baseline.json').write_text(json.dumps(after,indent=1)+'\n')
if '--write' in sys.argv:(ROOT/'tests/oracle/clojure-test-suite-known-failures.json').write_text(json.dumps(after,indent=1)+'\n')
print(counts);print('94 recovered failures +32 newly executed primary matches;4 Decimal failures retained; no previous passing assertion regresses.')
