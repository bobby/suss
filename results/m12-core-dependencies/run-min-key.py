"""Run the shipped min-key regression without changing primary/body/budgets."""
from pathlib import Path
import hashlib,json,os,subprocess,sys,time
ROOT=Path(__file__).resolve().parents[2];BASE=Path(__file__).resolve().parent;OUT=BASE/sys.argv[1]
OUT.mkdir(exist_ok=True);assert not (OUT/'command.json').exists()
command=['cargo','test','--workspace','--locked','--test','clojure_min_key',*sys.argv[2:],'--','--test-threads=2']
paths=subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z'],cwd=ROOT).decode().split('\0')
inputs={p:hashlib.sha256((ROOT/p).read_bytes()).hexdigest() for p in paths if p and not p.startswith('results/') and (ROOT/p).is_file()}
for name in ['cases.json','probe-cases.json','upstream-min-key.clj','min-key-adapted.sus','min-key.patch','adaptation.json','probe-equivalence-command.json']:
 p=BASE/name;inputs[str(p.relative_to(ROOT))]=hashlib.sha256(p.read_bytes()).hexdigest()
env=os.environ|{'CARGO_TARGET_DIR':str(ROOT.parent/'m12-suite-platform-target'),'CARGO_BUILD_JOBS':'1','RUSTUP_TOOLCHAIN':'stable','XDG_RUNTIME_DIR':'/run/user/1000'}
record={'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'command':command,'inputs':inputs,'complete':False}
def save():(OUT/'command.json').write_text(json.dumps(record,indent=2)+'\n')
save();started=time.time()
with (OUT/'run.log').open('w') as log:
 result=subprocess.run(['systemd-run','--user','--scope','-p','MemoryMax=3G','--',*command],cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT)
record.update(complete=True,exit_code=result.returncode,seconds=time.time()-started,inputs_unchanged=all(hashlib.sha256((ROOT/p).read_bytes()).hexdigest()==h for p,h in inputs.items()));save()
print({k:v for k,v in record.items() if k!='inputs'},flush=True);raise SystemExit(result.returncode)
