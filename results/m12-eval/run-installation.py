"""Execute compiler reentry and capability transport without claiming public eval."""
from pathlib import Path
import hashlib,json,os,signal,subprocess,sys,time
signal.signal(signal.SIGHUP,signal.SIG_IGN)
root=Path(__file__).resolve().parents[2];out=Path(__file__).resolve().parent/sys.argv[1]
out.mkdir(exist_ok=True);assert not (out/'commands.json').exists()
scope=os.environ['SUSS_DURABLE_SCOPE'];assert scope in Path('/proc/self/cgroup').read_text()
env=os.environ|{'CARGO_TARGET_DIR':os.environ['SUSS_EVAL_TARGET'],'CARGO_BUILD_JOBS':'1','RUSTUP_TOOLCHAIN':'stable'}
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
paths=subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z'],cwd=root).decode().split('\0')
inputs={p:sha(root/p) for p in paths if p and not p.startswith('results/') and (root/p).is_file()}
record={'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),'inputs':inputs,'runner_sha256':sha(Path(__file__)),'scope':scope,'recorder_pid':os.getpid(),'commands':[],'complete':False}
def announce(value):
 with (out/"recorder.log").open("a") as progress:print(value,file=progress,flush=True)
def save():(out/'commands.json').write_text(json.dumps(record,indent=2)+'\n')
commands=[('runtime-capability',['cargo','test','--workspace','--locked','--test','runtime_evaluation','--','--nocapture','--test-threads=2']),('borrowed-store-compiler-reentry',['cargo','test','--workspace','--locked','--lib','borrowed_store_','--','--nocapture','--test-threads=2'])]
save();code=0
for label,command in commands:
 start=time.time()
 with (out/(label+'.log')).open('w') as log:result=subprocess.run(command,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
 entry={'label':label,'command':command,'exit_code':result.returncode,'seconds':time.time()-start};record['commands'].append(entry);save();announce(entry)
 if result.returncode:code=result.returncode;break
record.update(complete=True,exit_code=code,inputs_unchanged=all((root/p).is_file() and sha(root/p)==h for p,h in inputs.items()));save();announce({k:v for k,v in record.items() if k not in ['inputs','commands']})
raise SystemExit(code)
