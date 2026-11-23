from pathlib import Path
import hashlib,json,os,subprocess,time
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
commands=[['python3','-m','unittest','discover','-s','scripts','-p','test_*.py']]+[['python3','scripts/'+s,*args] for s,args in [('cljs_inventory.py',['--check']),('cljs_reviews.py',[]),('core_import.py',['--check']),('reader_core_catalog.py',['--check']),('apply_bootstrap.py',['--check']),('sequence_provenance.py',[]),('check_transducer_provenance.py',[]),('check_when_let_provenance.py',[]),('bitwise_provenance.py',[]),('wasi_lock.py',[]),('numeric_runtime.py',['--check']),('jvm_array_oracle.py',['--check']),('clojure_test_suite.py',[]),('clojure_test_suite.py',['overlap'])]]
paths=subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z'],cwd=ROOT).decode().split('\0');inputs={p:hashlib.sha256((ROOT/p).read_bytes()).hexdigest() for p in paths if p and not p.startswith('results/') and (ROOT/p).is_file()}
record={'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'commands':[],'inputs':inputs,'complete':False}
def save():(OUT/'foundation.json').write_text(json.dumps(record,indent=2)+'\n')
save()
for i,cmd in enumerate(commands):
 start=time.time()
 with (OUT/f'foundation-{i:02}.log').open('w') as log:p=subprocess.run(cmd,cwd=ROOT,stdout=log,stderr=subprocess.STDOUT)
 row={'command':cmd,'exit_code':p.returncode,'seconds':time.time()-start};record['commands'].append(row);save();print(row,flush=True)
 if p.returncode:break
record.update(complete=True,all_passed=len(record['commands'])==len(commands) and all(x['exit_code']==0 for x in record['commands']),inputs_unchanged=all(hashlib.sha256((ROOT/p).read_bytes()).hexdigest()==h for p,h in inputs.items()));save()
