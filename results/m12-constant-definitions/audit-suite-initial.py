from pathlib import Path
import sys,json,hashlib,collections
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
sys.path.insert(0,str(ROOT/'scripts'))
import clojure_test_suite as suite
reference=json.loads((ROOT/'tests/oracle/clojure-test-suite-observations.json').read_text())
observed=json.loads((OUT/'suite-capture/suite-observations.json').read_text())
old=json.loads((ROOT/'tests/oracle/clojure-test-suite-known-failures.json').read_text())
counts,new=suite.compare(reference,observed)
newly_loaded=set(old['namespaces'])-set(new['namespaces'])
changed_assertions=[]
for identity in set(old['assertions'])|set(new['assertions']):
 if old['assertions'].get(identity)!=new['assertions'].get(identity):
  changed_assertions.append({'id':identity,'old':old['assertions'].get(identity),'new':new['assertions'].get(identity)})
unexpected=[r for r in changed_assertions if r['id'].split('/')[0] not in newly_loaded]
raw={f"{r['test']}#{r['ordinal']}":r for r in observed['assertions']}
rows=[];per_ns=collections.defaultdict(collections.Counter)
for primary in reference['assertions']:
 identity=primary['id'];ns=primary['test'].split('/')[0]
 if ns not in newly_loaded:continue
 reason=new['assertions'].get(identity)
 if reason:
  outcome='not-executed' if reason.startswith(('namespace:','test:','missing')) else 'fail'
 else:
  assert identity in raw,identity
  reason,judged=suite.decide(primary,raw[identity]);assert not reason,(identity,reason)
  outcome='guest-judged' if judged else 'pass'
 per_ns[ns][outcome]+=1
 rows.append({'id':identity,'outcome':outcome,'reason':new['assertions'].get(identity),'primary':primary,'observed':raw.get(identity)})
report={'capture_sha256':hashlib.sha256((OUT/'suite-capture/suite-observations.json').read_bytes()).hexdigest(),'counts':counts,'newly_loaded':sorted(newly_loaded),'per_namespace':dict(sorted(per_ns.items())),'changed_assertions_outside_newly_loaded_namespaces':unexpected,'changed_namespace_diagnostics':{ns:{'old':old['namespaces'].get(ns),'new':v} for ns,v in new['namespaces'].items() if old['namespaces'].get(ns)!=v}}
(OUT/'suite-audit.json').write_text(json.dumps(report,indent=2)+'\n')
(OUT/'newly-executed-rows.json').write_text(json.dumps(rows,indent=2)+'\n')
# This is a review artifact, not a replacement of the checked-in baseline.
(OUT/'proposed-known-failures.json').write_text(json.dumps(new,indent=1,sort_keys=True)+'\n')
print('counts',counts,'newly loaded',len(newly_loaded),'changed old loaded assertions',len(unexpected))
for ns,summary in sorted(per_ns.items()):
 if any(ns.endswith('.'+x) for x in ['integer-qmark','ratio-qmark','rational-qmark','plus-squote','star-squote']):print(ns,dict(summary))
