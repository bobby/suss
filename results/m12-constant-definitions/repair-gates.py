from pathlib import Path
import hashlib,json,os,subprocess,time
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
ENV=os.environ|{'XDG_RUNTIME_DIR':'/run/user/1000','CARGO_BUILD_JOBS':'2','RUSTUP_TOOLCHAIN':'stable','CARGO_TARGET_DIR':str(ROOT.parent/'m12-helper-target')}
paths=subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z'],cwd=ROOT).decode().split('\0')
inputs={p:hashlib.sha256((ROOT/p).read_bytes()).hexdigest() for p in paths if p and not p.startswith('results/') and (ROOT/p).is_file()}
record={'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'inputs':inputs,'commands':[],'complete':False}
def save():(OUT/'repair-gates.json').write_text(json.dumps(record,indent=2)+'\n')
commands=[('repaired-definitions',['cargo','test','-p','suss-compile','--locked','--test','portable_definitions','--test','portable_resolution','--','--test-threads=2']),('reviewed-suite-compare',['python3','scripts/clojure_test_suite.py','compare','--observations','results/m12-constant-definitions/final/suite-capture/suite-observations.json']),('suite-comparator-tests',['python3','scripts/test_clojure_test_suite.py'])]
save()
for label,args in commands:
 start=time.time()
 with (OUT/(label+'.log')).open('w') as log:p=subprocess.run(['systemd-run','--user','--scope','-p','MemoryMax=4G','--',*args],cwd=ROOT,env=ENV,stdout=log,stderr=subprocess.STDOUT)
 record['commands'].append({'label':label,'command':args,'exit_code':p.returncode,'seconds':time.time()-start});save();print(record['commands'][-1],flush=True)
record.update(complete=True,inputs_unchanged=all(hashlib.sha256((ROOT/f).read_bytes()).hexdigest()==v for f,v in inputs.items()));save()
