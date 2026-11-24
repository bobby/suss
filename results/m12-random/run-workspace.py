"""Exact full baseline; recorder and native child share the durable bounded scope."""
from pathlib import Path
import hashlib,json,os,signal,subprocess,sys,time
signal.signal(signal.SIGHUP,signal.SIG_IGN)
root=Path(__file__).resolve().parents[2];out=Path(__file__).resolve().parent/sys.argv[1]
out.mkdir(exist_ok=True);assert not (out/'command.json').exists()
scope=os.environ['SUSS_DURABLE_SCOPE'];assert scope in Path('/proc/self/cgroup').read_text()
subprocess.run(['git','diff','--quiet'],cwd=root,check=True)
subprocess.run(['git','diff','--cached','--quiet'],cwd=root,check=True)
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
paths=subprocess.check_output(['git','ls-files','-z'],cwd=root).decode().split('\0')
inputs={p:sha(root/p) for p in paths if p and (root/p).is_file()}
core='clojurescript/src/main/cljs/cljs/core.cljs';inputs[core]=sha(root/core)
env=os.environ|{'CARGO_TARGET_DIR':os.environ['SUSS_RANDOM_TARGET'],'CARGO_BUILD_JOBS':'1','RUSTUP_TOOLCHAIN':'stable','XDG_RUNTIME_DIR':'/run/user/1000'}
command=['cargo','test','--workspace','--locked','--','--test-threads=2']
head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
r={'head':head,'command':command,'inputs':inputs,'scope':scope,'recorder_pid':os.getpid(),'runner_sha256':sha(Path(__file__)),'TMPDIR':env.get('TMPDIR'),'rustc':subprocess.check_output(['rustc','--version'],env=env,text=True).strip(),'complete':False}
def save():(out/'command.json').write_text(json.dumps(r,indent=2)+'\n')
save();start=time.time()
with (out/'run.log').open('w') as log:result=subprocess.run(command,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
r.update(complete=True,exit_code=result.returncode,seconds=time.time()-start,inputs_unchanged=all((root/p).is_file() and sha(root/p)==h for p,h in inputs.items()),head_unchanged=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()==head);save();print({k:v for k,v in r.items() if k!='inputs'},flush=True)
raise SystemExit(result.returncode)
