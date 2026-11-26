"""Check source-qualified supplementary scope references, preserving raw oracles."""
from pathlib import Path
import gzip,hashlib,json,subprocess
ROOT=Path(__file__).resolve().parents[2];BASE=Path(__file__).resolve().parent;P=BASE/'portable-dynamic-primary'
sha=lambda raw:hashlib.sha256(raw).hexdigest();command=json.loads((P/'command.json').read_bytes());rows=json.loads((P/'cases.json').read_bytes())
assert command['exit_code']==0 and command['controls']==len(rows)==2
assert all(sha(Path(p).read_bytes())==h for p,h in command['jars'].items())
assert sha((P/'oracle.clj').read_bytes())==command['source_sha256']
assert sha((P/'stdout.log').read_bytes())==command['stdout_sha256']
assert [json.loads(x) for x in (P/'stdout.log').read_text().splitlines()]==[r['expected'] for r in rows]
corpus=ROOT/'tests/oracle/portable-cases.json';raw=corpus.read_bytes();assert sha(raw)==command['immutable_corpus_sha256'];data=json.loads(raw)
assert data['schema']==1 and data['upstream']=='c4295f303100bbf5afac449242d30bca1126f1a1' and len(data['cases'])==397
assert [r['id'] for r in rows]==['dynamic-snapshot-before-initializers','dynamic-definition-inside-binding']
for row in rows:
 original=next(x for x in data['cases'] if x['id']==row['id'])
 assert row['source']==original['source'] and row['legacy_cljs_expected']==original['expected']
assert rows[1]['source']==rows[1]['primary_source']
expected_oracle='(assert (= "1.12.6" (clojure-version)))\n'+''.join('(let [value '+r['primary_source']+'] (assert (instance? Long value)) (println (str "{\\\"tag\\\":\\\"long\\\",\\\"value\\\":\\\"" value "\\\"}")))\n' for r in rows)
assert (P/'oracle.clj').read_text()==expected_oracle
legacy=json.loads((P/'legacy-root-set-command.json').read_bytes());assert legacy['exit_code']==1
assert sha((P/'legacy-root-set.clj').read_bytes())==legacy['source_sha256']
assert sha((P/'legacy-root-set.stderr').read_bytes())==legacy['stderr_sha256']
assert "Can't change/establish root binding" in (P/'legacy-root-set.stderr').read_text()
# Only the corpus function and exact module trace hook change after the318-test gate.
focused=json.loads(gzip.decompress((BASE/'with-redefs-root-focused-after-ir-kind/commands.json.gz').read_bytes()))
assert focused['complete'] and focused['inputs_unchanged'] and focused['head_unchanged'] and all(c['exit_code']==0 for c in focused['commands'])
test='crates/suss-compile/tests/portable_pipeline.rs';prose={'results/m12-eval/README.md','docs/compatibility/README.md','docs/roadmap/handoff.md','docs/runtime/dynamic-bindings.md'}
assert all((ROOT/p).is_file() and sha((ROOT/p).read_bytes())==h for p,h in focused['inputs'].items() if p not in prose|{test,'crates/suss-compile/tests/portable_modules.rs'})
module='crates/suss-compile/tests/portable_modules.rs'
old_module=subprocess.check_output(['git','show','c9eecf80df791baba8fc51c19cc7d6e1ea136179:'+module],cwd=ROOT)
assert old_module.count(b'\"binding-set\"')==2
assert (ROOT/module).read_bytes()==old_module.replace(b'\"binding-set\"',b'\"binding-root-set\"')
old=subprocess.check_output(['git','show','c9eecf80df791baba8fc51c19cc7d6e1ea136179:'+test],cwd=ROOT).decode();new=(ROOT/test).read_text();start='fn compiled_source_cases_match_the_pinned_compiler_observations()';end='\n/// Where Clojure'
a,b=old.split(start,1);x,y=b.split(end,1);aa,bb=new.split(start,1);xx,yy=bb.split(end,1);assert a==aa and y==yy
for exact in ['assert!(failing.is_empty(), "{failing:#?}");','assert_eq!(ids.len(), 397);','assert_eq!(corrected, 142);','let observed = try_execute(&source)']:
 assert exact in x and exact in xx
assert 'assert_eq!(dynamic_references, 2);' in xx
assert sha((ROOT/'tests/oracle/clojure-test-suite-observations.json').read_bytes())=='449264ab6bca1dcf761d0204e4a64de4de91562e576615c4e84a63246839f4c7'
assert sha((ROOT/'tests/oracle/clojure-test-suite-known-failures.json').read_bytes())=='dea5db93e9548008d91e5c79ee11978920575975ec601f1852139ff063329d7f'
print('Two actual supplementary scope references verified; raw397 corpus/142 corrections and all original primary/baseline rules unchanged.318-tested production/source inputs conserved except the corpus function and exact two-string module trace adjustment, each requiring its own executing gate. Legacy root-set JVM1 is explicit, not a primary match.')
