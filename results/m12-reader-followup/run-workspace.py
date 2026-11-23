"""Required unfiltered baseline; freeze every tracked input, including fixtures."""
from pathlib import Path
import hashlib,json,os,subprocess,time,sys
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent/(sys.argv[1] if len(sys.argv)>1 else 'workspace-baseline');OUT.mkdir(exist_ok=True);assert not (OUT/'command.json').exists()
subprocess.run(['git','diff','--quiet'],cwd=ROOT,check=True);subprocess.run(['git','diff','--cached','--quiet'],cwd=ROOT,check=True)
paths=subprocess.check_output(['git','ls-files','-z'],cwd=ROOT).decode().split('\0');inputs={p:hashlib.sha256((ROOT/p).read_bytes()).hexdigest() for p in paths if p and (ROOT/p).is_file()}
core='clojurescript/src/main/cljs/cljs/core.cljs';inputs[core]=hashlib.sha256((ROOT/core).read_bytes()).hexdigest()
command=['cargo','test','--workspace','--locked','--','--test-threads=2'];scope=f'suss-m12-reader-baseline-{os.getpid()}.scope'
env=os.environ|{'CARGO_TARGET_DIR':str(ROOT.parent/'m12-suite-platform-target'),'CARGO_BUILD_JOBS':'1','RUSTUP_TOOLCHAIN':'stable','XDG_RUNTIME_DIR':'/run/user/1000'}
record={'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'command':command,'scope':scope,'inputs':inputs,'harness_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'rustc':subprocess.check_output(['rustc','--version'],cwd=ROOT,env=env,text=True).strip(),'complete':False}
def save():(OUT/'command.json').write_text(json.dumps(record,indent=2)+'\n')
save();started=time.time()
with (OUT/'run.log').open('w') as log:r=subprocess.run(['systemd-run','--user','--scope','--unit',scope,'-p','MemoryMax=4G','--',*command],cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT)
record.update(complete=True,exit_code=r.returncode,seconds=time.time()-started,inputs_unchanged=all((ROOT/p).is_file() and hashlib.sha256((ROOT/p).read_bytes()).hexdigest()==h for p,h in inputs.items()));save();print({k:v for k,v in record.items() if k!='inputs'},flush=True);raise SystemExit(r.returncode)
