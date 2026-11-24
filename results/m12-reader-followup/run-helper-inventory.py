"""Run the original unboxed-locals suite with the explicit appended ABI names."""
from pathlib import Path
import hashlib,json,os,subprocess,time
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent/'after-explicit-character-helper-inventory'
OUT.mkdir(exist_ok=True);assert not (OUT/'commands.json').exists()
paths=subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z'],cwd=ROOT).decode().split('\0')
inputs={p:hashlib.sha256((ROOT/p).read_bytes()).hexdigest() for p in paths if p and not p.startswith('results/') and (ROOT/p).is_file()}
command=['cargo','test','--workspace','--locked','--test','portable_unboxed_locals','--','--test-threads=2']
env=os.environ|{'CARGO_TARGET_DIR':str(ROOT.parent/'m12-review-repair-target'),'CARGO_BUILD_JOBS':'1','RUSTUP_TOOLCHAIN':'stable','XDG_RUNTIME_DIR':'/run/user/1000'}
record={'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'command':command,'inputs':inputs,'complete':False}
def save():(OUT/'commands.json').write_text(json.dumps(record,indent=2)+'\n')
save();started=time.time()
with (OUT/'run.log').open('w') as log:
 result=subprocess.run(['systemd-run','--user','--scope','-p','MemoryMax=3G','--',*command],cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT)
record.update(complete=True,exit_code=result.returncode,seconds=time.time()-started,inputs_unchanged=all(hashlib.sha256((ROOT/p).read_bytes()).hexdigest()==h for p,h in inputs.items()));save()
print({k:v for k,v in record.items() if k!='inputs'},flush=True);raise SystemExit(result.returncode)
