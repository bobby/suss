"""Regress existing owned-session/module behavior after shared publication."""
from pathlib import Path
import hashlib,json,os,signal,subprocess,sys,time
signal.signal(signal.SIGHUP,signal.SIG_IGN)
root=Path(__file__).resolve().parents[2];out=Path(__file__).resolve().parent/sys.argv[1]
out.mkdir(exist_ok=True);assert not (out/'commands.json').exists()
scope=os.environ['SUSS_DURABLE_SCOPE'];assert scope in Path('/proc/self/cgroup').read_text()
env=os.environ|{'CARGO_TARGET_DIR':os.environ['SUSS_EVAL_TARGET'],'CARGO_BUILD_JOBS':'1','RUSTUP_TOOLCHAIN':'stable','XDG_RUNTIME_DIR':'/run/user/1000'}
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
paths=subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z'],cwd=root).decode().split('\0')
inputs={p:sha(root/p) for p in paths if p and not p.startswith('results/') and (root/p).is_file()}
r={'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),'inputs':inputs,'runner_sha256':sha(Path(__file__)),'scope':scope,'recorder_pid':os.getpid(),'commands':[],'complete':False}
def save():(out/'commands.json').write_text(json.dumps(r,indent=2)+'\n')
def announce(value):
 with (out/'recorder.log').open('a') as log:print(value,file=log,flush=True)
commands=[('existing-session-library',['cargo','test','--workspace','--locked','--lib','portable_session::','--','--nocapture','--test-threads=2']),('inline-module-macros',['cargo','test','--workspace','--locked','--test','inline_module_macros','--','--nocapture','--test-threads=2']),('isolated-observation-hooks',['cargo','test','--workspace','--locked','--test','session_observation_hook','--','--nocapture','--test-threads=2']),('java-node-free-bootstrap',['sh','scripts/verify-bootstrap.sh'])]
save();code=0
for label,command in commands:
 start=time.time()
 with (out/(label+'.log')).open('w') as log:result=subprocess.run(command,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
 entry={'label':label,'command':command,'exit_code':result.returncode,'seconds':time.time()-start};r['commands'].append(entry);save();announce(entry)
 if result.returncode:code=result.returncode;break
r.update(complete=True,exit_code=code,inputs_unchanged=all((root/p).is_file() and sha(root/p)==h for p,h in inputs.items()));save();announce({k:v for k,v in r.items() if k not in ['inputs','commands']})
raise SystemExit(code)
