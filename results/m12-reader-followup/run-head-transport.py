"""Execute the real compiled head transport and unchanged comparator controls."""
from pathlib import Path
import hashlib,json,os,subprocess,time,sys
ROOT=Path(__file__).resolve().parents[2]
OUT=Path(__file__).resolve().parent/(sys.argv[1] if len(sys.argv)>1 else 'reserved-core-head-transport')
OUT.mkdir(exist_ok=True);assert not (OUT/'command.json').exists()
paths=subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z'],cwd=ROOT).decode().split('\0')
inputs={p:hashlib.sha256((ROOT/p).read_bytes()).hexdigest() for p in paths if p and not p.startswith('results/') and (ROOT/p).is_file()}
command=['cargo','test','-p','suss-compile','--locked','--test','clojure_test_suite','assertion_head_transport','--','--nocapture','--test-threads=2']
record={'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'command':command,'inputs':inputs,'complete':False}
def save():(OUT/'command.json').write_text(json.dumps(record,indent=2)+'\n')
env=os.environ|{'CARGO_TARGET_DIR':str(ROOT.parent/'m12-case-followup-target'),'CARGO_BUILD_JOBS':'1','RUSTUP_TOOLCHAIN':'stable','XDG_RUNTIME_DIR':'/run/user/1000'}
save();started=time.time()
with (OUT/'run.log').open('w') as log:
 result=subprocess.run(['systemd-run','--user','--scope','-p','MemoryMax=4G','--',*command],cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT)
record.update(complete=True,exit_code=result.returncode,seconds=time.time()-started,inputs_unchanged=all(hashlib.sha256((ROOT/p).read_bytes()).hexdigest()==h for p,h in inputs.items()))
save();print({k:v for k,v in record.items() if k!='inputs'},flush=True)
raise SystemExit(result.returncode)
