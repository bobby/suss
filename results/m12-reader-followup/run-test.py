"""Record one focused cargo test without altering oracle or baseline controls."""
from pathlib import Path
import hashlib,json,os,subprocess,sys,time
ROOT=Path(__file__).resolve().parents[2]
OUT=Path(__file__).resolve().parent/sys.argv[1]
OUT.mkdir(parents=True,exist_ok=True)
assert not (OUT/'commands.json').exists()
paths=subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z'],cwd=ROOT).decode().split('\0')
inputs={p:hashlib.sha256((ROOT/p).read_bytes()).hexdigest() for p in paths if p and not p.startswith('results/') and (ROOT/p).is_file()}
command=['cargo','test','--workspace','--locked',*sys.argv[2:],'--','--test-threads=2']
record={'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'inputs':inputs,'command':command,'complete':False}
def save():(OUT/'commands.json').write_text(json.dumps(record,indent=2)+'\n')
save();start=time.time()
env=os.environ|{'CARGO_TARGET_DIR':str(ROOT.parent/'m12-suite-platform-target'),'CARGO_BUILD_JOBS':'1','RUSTUP_TOOLCHAIN':'stable','XDG_RUNTIME_DIR':'/run/user/1000'}
with (OUT/'run.log').open('w') as log:
 p=subprocess.run(['systemd-run','--user','--scope','-p','MemoryMax=3G','--',*command],cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT)
record.update(complete=True,exit_code=p.returncode,seconds=time.time()-start,inputs_unchanged=all((ROOT/f).is_file() and hashlib.sha256((ROOT/f).read_bytes()).hexdigest()==h for f,h in inputs.items()))
save();print({k:v for k,v in record.items() if k!='inputs'},flush=True)
raise SystemExit(p.returncode)
