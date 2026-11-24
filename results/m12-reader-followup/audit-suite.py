"""Compare a complete capture against the unchanged primary and parent baseline.

No baseline write occurs here. Counts, namespace/test failures and skips retain
all original comparator rules; numerical subsets require actual observations.
"""
from pathlib import Path
import collections,gzip,hashlib,json,subprocess,sys
ROOT=Path(__file__).resolve().parents[2];BASE='a783ad60540b4bc22820e8811a568132a0b14210'
OUT=Path(__file__).resolve().parent/sys.argv[1]
sys.path.insert(0,str(ROOT/'scripts'));import clojure_test_suite as suite

def original(path):return subprocess.check_output(['git','show',BASE+':'+path],cwd=ROOT)
def sha(raw):return hashlib.sha256(raw).hexdigest()
primary_path='tests/oracle/clojure-test-suite-observations.json'
primary=(ROOT/primary_path).read_bytes();assert primary==original(primary_path)
for path in ['scripts/clojure_test_suite.py','tests/oracle/clojure-test-suite-suite.json']:
 if (ROOT/path).exists():assert (ROOT/path).read_bytes()==original(path),path
path='crates/suss-compile/tests/clojure_test_suite.rs';old_test=original(path).decode()
projection='        Observation::Uuid { most, least } => json!({"tag":"uuid", "most":most.to_string(), "least":least.to_string()}),\n'
expected_test=old_test.replace('        Observation::BigInt(value)',projection+'        Observation::BigInt(value)',1)
setup='''        // Local transformers import this namespace in the isolated Macro phase.
        // Provide that dependency explicitly, without sharing Runtime's Store,
        // decoder, snapshots or token stream. Assertions execute only in Runtime;
        // an accidental expansion-time capture is a hard failure, never a token.
        macros.install_observation_hook(|_, _| Err(wasmtime::Error::msg(
            "Assertion snapshots require Runtime test execution",
        ))).unwrap();
'''
anchor='        macros.set_operation_fuel(OPERATION_FUEL);\n'
assert expected_test.count(anchor)==1
expected_test=expected_test.replace(anchor,anchor+setup,1)
expected_test=expected_test.replace('mod compilation_fuel;\n','mod compilation_fuel;\n#[path = "support/assertion_head_transport.rs"]\nmod assertion_head_transport;\n',1)
expected_test=expected_test.replace('mod assertion_head_transport;\n','mod assertion_head_transport;\n#[path = "support/pinned_min_key_body.rs"]\nmod pinned_min_key_body;\n',1)
expected_test=expected_test.replace('            let head = name(&fields[3]);','            let source_head = name(&fields[3]);\n            let head = source_head.as_deref().map(assertion_head_transport::canonical);',1)
expected_test=expected_test.replace('"verdict":verdict,"head":head','"verdict":verdict,"head":head,"source-head":source_head',1)
assert (ROOT/path).read_text()==expected_test,'only typed UUID projection, explicit Macro service, raw-preserving core-head transport and focused min-key test module inclusion may differ'
previous_bytes=gzip.decompress(original('results/m12-case-dispatch/suite-after-manifest-refresh/suite-capture/suite-observations.json.gz'))
previous=json.loads(previous_bytes);reference=json.loads(primary)
old_baseline=original('tests/oracle/clojure-test-suite-known-failures.json');old=json.loads(old_baseline)
old_counts,measured=suite.compare(reference,previous);assert measured==old
observed_bytes=(OUT/'suite-capture/suite-observations.json').read_bytes();observed=json.loads(observed_bytes)
transported=[]
for row in observed['assertions']:
 raw=row['source-head'];canonical=raw
 if raw is not None and raw.partition('/')[0] in ['cljs.core','suss.core'] and '/' in raw:
  canonical='clojure.core/'+raw.partition('/')[2]
 assert row['head']==canonical,(row['test'],row['ordinal'],raw,row['head'])
 if canonical!=raw:transported.append({'test':row['test'],'ordinal':row['ordinal'],'source-head':raw,'head':canonical})
counts,new=suite.compare(reference,observed)
assert new['skip-mismatches']==old['skip-mismatches'],'skip rules/outcomes changed'
assert [(r['namespace']) for r in observed['namespaces']]==reference['namespaces'],'namespace coverage changed'

def identities(rows):return {f"{r['test']}#{r['ordinal']}":r for r in rows}
actual=identities(observed['assertions']);before=identities(previous['assertions']);rows={r['id']:r for r in reference['assertions']}
expected_count=collections.Counter(r['test'] for r in reference['assertions'])
def outcome(row,actual,failures):
 identity=row['id'];namespace=row['test'].split('/')[0]
 if namespace in failures['namespaces']:return 'not-executed'
 reason=failures['assertions'].get(identity)
 if reason:return 'not-executed' if reason.startswith(('namespace:','test:','missing')) else 'fail'
 assert identity in actual,(identity,'absent results are not successes')
 assert sum(x['test']==row['test'] for x in actual.values())==expected_count[row['test']],identity
 reason,judged=suite.decide(row,actual[identity]);assert reason is None,(identity,reason)
 return 'guest-judged' if judged else 'pass'
old_passes={identity for identity,row in rows.items() if outcome(row,before,old) in ['pass','guest-judged']}
assert len(old_passes)==old_counts['pass']+old_counts['guest-judged']
regressions=[identity for identity in sorted(old_passes) if outcome(rows[identity],actual,new) not in ['pass','guest-judged']]
assert not regressions,regressions
raw_passes={identity for identity,row in rows.items() if identity in before and suite.decide(row,before[identity])[0] is None}
raw_regressions=[identity for identity in sorted(raw_passes) if identity not in actual or suite.decide(rows[identity],actual[identity])[0] is not None]
assert not raw_regressions,raw_regressions

def tagged(node,tags):
 if isinstance(node,dict):return node.get('tag') in tags or any(tagged(v,tags) for v in node.values())
 if isinstance(node,list):return any(tagged(v,tags) for v in node)
 return False
subsets={}
for name,tags in [('decimal',{'decimal'}),('bigint-ratio',{'bigint','biginteger','ratio'})]:
 selected=[row for row in reference['assertions'] if tagged(row,tags)]
 if name=='decimal':assert len(selected)==200
 subsets[name]={'primary_assertions':len(selected),'outcomes':dict(collections.Counter(outcome(row,actual,new) for row in selected)),
  'unresolved':[{'id':row['id'],'form':row['form'],'outcome':outcome(row,actual,new),
    'reason':new['assertions'].get(row['id']) or new['namespaces'].get(row['test'].split('/')[0]) or new['tests'].get(row['test']),
    'actual':actual.get(row['id'])} for row in selected if outcome(row,actual,new) not in ['pass','guest-judged']]}
changes={key:[{'id':identity,'old':old[key].get(identity),'new':new[key].get(identity)} for identity in sorted(set(old[key])|set(new[key])) if old[key].get(identity)!=new[key].get(identity)] for key in ['assertions','tests','namespaces']}
newly_executed=[];other_new=[]
for identity in new['assertions'].keys()-old['assertions'].keys():
 row=rows[identity];item={'id':identity,'form':row['form'],'reason':new['assertions'][identity],'actual':actual.get(identity)}
 if identity not in before and (row['test'].split('/')[0] in old['namespaces'] or row['test'] in old['tests']):newly_executed.append(item)
 else:other_new.append(item)
edn='clojure.edn-test.read-string';edn_rows=[row for row in reference['assertions'] if row['test'].split('/')[0]==edn]
report={'base':BASE,'primary_sha256':sha(primary),'previous_capture_sha256':sha(previous_bytes),'capture_sha256':sha(observed_bytes),
 'raw_assertions':len(observed['assertions']),'old_counts':old_counts,'counts':counts,'changes':changes,
 'prior_strict_passes_preserved':len(old_passes),'prior_individually_comparable_passes_preserved':len(raw_passes),
 'previously_passing_regressions':regressions,'raw_comparison_regressions':raw_regressions,
 'newly_executed_failures':newly_executed,'other_new_failures':other_new,
 'numeric_subsets':subsets,'edn':{'primary_assertions':len(edn_rows),'captured_assertions':sum(r['test'].split('/')[0]==edn for r in actual.values()),'outcomes':dict(collections.Counter(outcome(row,actual,new) for row in edn_rows))},
 'primary_and_count_skip_guards_unchanged':True,'baseline_written':False,
 'core_alias_head_transport':transported,
 'harness_setup_change':'Explicit isolated Macro observer always raises; reserved core head spellings use clojure.core with raw source-head retained; Runtime decoder/snapshots/tokens and every suite body/routing/comparison remain unchanged'}
for name,value in [('suite-audit.json',report),('proposed-known-failures.json',new)]:
 path=OUT/name;assert not path.exists(),path;path.write_text(json.dumps(value,indent=2,sort_keys=True)+'\n')
(OUT/'reviewed-before-known-failures.json.gz').write_bytes(gzip.compress(old_baseline,mtime=0))
print({k:v for k,v in report.items() if k not in ['changes','numeric_subsets','newly_executed_failures','other_new_failures','core_alias_head_transport']})
print('numeric outcomes:',{k:v['outcomes'] for k,v in subsets.items()})
