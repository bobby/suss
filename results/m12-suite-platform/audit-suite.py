"""Audit every changed failure without weakening primary values or count guards."""
from pathlib import Path
import ast,collections,gzip,hashlib,json,re,subprocess,sys
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent/'suite-after-platform'
sys.path.insert(0,str(ROOT/'scripts'));import clojure_test_suite as suite
BASE='a12b279615c23cb18f58a80e0255e63b4ae4e873'
def original(path):return subprocess.check_output(['git','show',BASE+':'+path],cwd=ROOT)
reference_path='tests/oracle/clojure-test-suite-observations.json';primary=(ROOT/reference_path).read_bytes();assert primary==original(reference_path)
# Every Python comparison/count/skip statement except suite_matches is unchanged.
path='scripts/clojure_test_suite.py'
def without_value_matcher(source):
 tree=ast.parse(source);tree.body=[n for n in tree.body if not (isinstance(n,ast.Assign) and any(isinstance(t,ast.Name) and t.id=='SUSS_PATCHES' for t in n.targets))];return ast.dump(tree,include_attributes=False)
assert without_value_matcher((ROOT/path).read_text())==without_value_matcher(original(path).decode())
# The capture and assertion-count/source-routing behavior are unchanged.
path='crates/suss-compile/tests/clojure_test_suite.rs';capture=(ROOT/path).read_text();assert capture.startswith(original(path).decode());assert capture[len(original(path).decode()):].lstrip().startswith("/// ADR-0003's scalar representations")
# The only script change is one registered source adaptation; all comparison,
# count, skip, source-routing and primary-oracle decisions are byte-equivalent AST.
def patch_names(source):
 tree=ast.parse(source);return next(ast.literal_eval(n.value) for n in tree.body if isinstance(n,ast.Assign) and any(isinstance(t,ast.Name) and t.id=='SUSS_PATCHES' for t in n.targets))
assert patch_names((ROOT/'scripts/clojure_test_suite.py').read_text())==patch_names(original('scripts/clojure_test_suite.py').decode())+('0007-narrow-cast-suss-representations.patch',)

raw=(OUT/'suite-capture/suite-observations.json').read_bytes();observed=json.loads(raw);reference=json.loads(primary)
base_path='tests/oracle/clojure-test-suite-known-failures.json';old_bytes=original(base_path);old=json.loads(old_bytes);counts,new=suite.compare(reference,observed)
assert old['skip-mismatches']==new['skip-mismatches']
actual={f"{x['test']}#{x['ordinal']}":x for x in observed['assertions']};primary_rows={x['id']:x for x in reference['assertions']}
changes={key:[{'id':k,'old':old[key].get(k),'new':new[key].get(k)} for k in sorted(set(old[key])|set(new[key])) if old[key].get(k)!=new[key].get(k)] for key in ['assertions','tests','namespaces']}
regressions=[k for k in new['assertions'] if k not in old['assertions']];assert not regressions,regressions
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
(OUT/'decimal-value-unexecuted.json').write_text(json.dumps(decimal_missing,indent=2)+'\n')
# A value-only subset is not evidence that every Decimal-related source assertion passes.
source_candidates=[]
for row in reference['assertions']:
 if re.search(r'(?:\d(?:\.\d*)?(?:[eE][+-]?\d+)?M\b|\b(?:bigdec|decimal\?|with-precision)\b)',row.get('form','')):
  reason=new['assertions'].get(row['id']) or new['namespaces'].get(row['test'].split('/')[0]) or new['tests'].get(row['test'])
  if reason:source_candidates.append({'id':row['id'],'form':row['form'],'reason':reason,'actual':actual.get(row['id'])})
report={'base':BASE,'checkpoint':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'capture_sha256':hashlib.sha256(raw).hexdigest(),'primary_sha256':hashlib.sha256(primary).hexdigest(),'raw_assertions':len(observed['assertions']),'counts':counts,'changed_record_counts':{k:len(v) for k,v in changes.items()},'changes':changes,'previously_passing_regressions':regressions,'primary_and_count_skip_guards_unchanged':True,'decimal_value_observations':dict(decimal_outcomes),'unresolved_decimal_source_candidates':len(source_candidates),'unresolved_candidates_by_namespace':dict(collections.Counter(x['id'].split('/')[0] for x in source_candidates))}
(OUT/'suite-audit.json').write_text(json.dumps(report,indent=2)+'\n');(OUT/'decimal-source-candidates.json').write_text(json.dumps(source_candidates,indent=2)+'\n');(OUT/'proposed-known-failures.json').write_text(json.dumps(new,indent=1,sort_keys=True)+'\n')
archive=OUT/'reviewed-before-known-failures.json.gz'
if not archive.exists():archive.write_bytes(gzip.compress(old_bytes,mtime=0))
assert gzip.decompress(archive.read_bytes())==old_bytes
if '--write' in sys.argv:
 assert (ROOT/base_path).read_bytes()==old_bytes,'baseline changed since reviewed base'
 command=['python3','scripts/clojure_test_suite.py','compare','--observations',str(OUT/'suite-capture/suite-observations.json'),'--write'];result=subprocess.run(command,cwd=ROOT,capture_output=True,text=True);(OUT/'reviewed-write.log').write_text(result.stdout+result.stderr);assert result.returncode==0
 command2=command[:-1];result2=subprocess.run(command2,cwd=ROOT,capture_output=True,text=True);(OUT/'reviewed-compare.log').write_text(result2.stdout+result2.stderr);assert result2.returncode==0
 assert json.loads((ROOT/base_path).read_bytes())==new
 (OUT/'reviewed-baseline-commands.json').write_text(json.dumps({'commands':[{'command':command,'exit_code':result.returncode},{'command':command2,'exit_code':result2.returncode}],'old_sha256':hashlib.sha256(old_bytes).hexdigest(),'new_sha256':hashlib.sha256((ROOT/base_path).read_bytes()).hexdigest()},indent=2)+'\n')
print({k:v for k,v in report.items() if k not in ['changes','unresolved_candidates_by_namespace']})
