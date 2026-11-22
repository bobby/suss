from pathlib import Path
import hashlib,json,os,subprocess,time
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
ENV=os.environ|{'CARGO_TARGET_DIR':str(ROOT.parent/'m12-helper-target'),'CARGO_BUILD_JOBS':'2','RUSTUP_TOOLCHAIN':'stable','XDG_RUNTIME_DIR':'/run/user/1000'}
commands=[('abi',['cargo','test','-p','suss-compile','--locked','--test','runtime_abi','--test','portable_calling_convention','--','--test-threads=2']),('numeric',['cargo','test','-p','suss-cli','--locked','--test','clojure_bigints','--test','clojure_bigint_integers','--test','clojure_ratios','--','--test-threads=2']),('numeric-rebuild',['python3','scripts/numeric_runtime.py','--rebuild-check']),('bootstrap',['sh','scripts/verify-bootstrap.sh'])]
inputs=json.loads((OUT/'inputs.json').read_text());record={'inputs':inputs,'commands':[],'complete':False}
def save():(OUT/'controls.json').write_text(json.dumps(record,indent=2)+'\n')
save()
for label,command in commands:
 start=time.time()
 with (OUT/(label+'.log')).open('w') as log:r=subprocess.run(['systemd-run','--user','--scope','-p','MemoryMax=4G','--',*command],cwd=ROOT,env=ENV,stdout=log,stderr=subprocess.STDOUT)
 record['commands'].append({'label':label,'command':command,'exit_code':r.returncode,'seconds':time.time()-start});save();print(label,r.returncode,flush=True)
 if r.returncode:raise SystemExit(r.returncode)
assert all(hashlib.sha256((ROOT/p).read_bytes()).hexdigest()==h for p,h in inputs.items())
record.update(complete=True,inputs_unchanged=True);save()
