from pathlib import Path
import collections,gzip,hashlib,json,sys
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
sys.path.insert(0,str(ROOT/'scripts'))
import clojure_test_suite as suite
proof=json.loads((OUT/'unchanged-oracle-inputs.json').read_bytes())
for p,h in proof['inputs'].items():assert hashlib.sha256((ROOT/p).read_bytes()).hexdigest()==h,p
reference=json.loads((ROOT/'tests/oracle/clojure-test-suite-observations.json').read_bytes())
capture=OUT/'suite-capture/suite-observations.json'
raw=capture.read_bytes() if capture.exists() else gzip.decompress(capture.with_suffix('.json.gz').read_bytes())
observed=json.loads(raw)
old=json.loads(gzip.decompress((OUT/'reviewed-before-known-failures.json.gz').read_bytes()))
counts,new=suite.compare(reference,observed)
changes={key:[{'id':k,'old':old[key].get(k),'new':new[key].get(k)} for k in sorted(set(old[key])|set(new[key])) if old[key].get(k)!=new[key].get(k)] for key in ['assertions','tests','namespaces']}
assert old['skip-mismatches']==new['skip-mismatches']
actual={f"{r['test']}#{r['ordinal']}":r for r in observed['assertions']}
primary={r['id']:r for r in reference['assertions']}
for row in changes['assertions']:
 row['primary']=primary.get(row['id']);row['observed']=actual.get(row['id'])
report={'capture_sha256':hashlib.sha256(raw).hexdigest(),'counts':counts,'changes':changes,'skip_mismatches_unchanged':True}
newns=set(old['namespaces'])-set(new['namespaces'])
perns=collections.defaultdict(collections.Counter);rows=[]
for p in reference['assertions']:
 ns=p['test'].split('/')[0]
 if ns not in newns:continue
 reason=new['assertions'].get(p['id'])
 if reason:outcome='not-executed' if reason.startswith(('namespace:','test:','missing')) else 'fail'
 else:
  reason,judged=suite.decide(p,actual[p['id']]);assert not reason
  outcome='guest-judged' if judged else 'pass'
 perns[ns][outcome]+=1
 rows.append({'id':p['id'],'outcome':outcome,'reason':reason,'primary':p,'observed':actual.get(p['id'])})
regressions=[r['id'] for r in changes['assertions'] if r['old'] is None and r['new'] is not None and r['id'].split('/')[0] not in newns]
assert not regressions,regressions
report.update(newly_loaded_namespace_results={k:dict(v) for k,v in sorted(perns.items())},regressed_previously_passing_assertions=regressions,reference_unchanged=True,comparison_and_count_guards_unchanged=True)
(OUT/'newly-executed-rows.json').write_text(json.dumps(rows,indent=2)+'\n')
(OUT/'suite-audit.json').write_text(json.dumps(report,indent=2)+'\n')
(OUT/'proposed-known-failures.json').write_text(json.dumps(new,indent=1,sort_keys=True)+'\n')
print(counts)
print({k:len(v) for k,v in changes.items()})
for row in changes['namespaces']:print(row)
