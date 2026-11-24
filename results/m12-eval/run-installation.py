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
if sys.argv[2:] == ["--borrowed-session-only"]:
 commands=[('borrowed-session-view',['cargo','test','--workspace','--locked','--lib','portable_session::reentry::tests','--','--nocapture','--test-threads=2'])]
elif sys.argv[2:] == ["--borrowed-analysis-only"]:
 commands=[('borrowed-analysis-view',['cargo','test','--workspace','--locked','--lib','portable_session::reentry::tests','--','--nocapture','--test-threads=2']),('analysis-graph-regressions',['cargo','test','--workspace','--locked','--lib','portable_macro_graph::','--','--nocapture','--test-threads=2'])]
elif sys.argv[2:] == ["--borrowed-macro-only"]:
 commands=[('borrowed-macro-authority',['cargo','test','--workspace','--locked','--lib','portable_macros::reentry::tests','--','--nocapture','--test-threads=2']),('existing-macro-regressions',['cargo','test','--workspace','--locked','--lib','portable_macros::','--','--nocapture','--test-threads=2']),('inline-module-macros',['cargo','test','--workspace','--locked','--test','inline_module_macros','--','--nocapture','--test-threads=2'])]
elif sys.argv[2:] == ["--macro-service-only"]:
 commands=[('runtime-macro-service',['cargo','test','--workspace','--locked','--lib','portable_macros::facade::tests','--','--nocapture','--test-threads=2']),('existing-macro-regressions',['cargo','test','--workspace','--locked','--lib','portable_macros::','--','--nocapture','--test-threads=2']),('inline-module-macros',['cargo','test','--workspace','--locked','--test','inline_module_macros','--','--nocapture','--test-threads=2']),('existing-session-library',['cargo','test','--workspace','--locked','--lib','portable_session::','--','--nocapture','--test-threads=2']),('isolated-observation-hooks',['cargo','test','--workspace','--locked','--test','session_observation_hook','--','--nocapture','--test-threads=2']),('java-node-free-bootstrap',['sh','scripts/verify-bootstrap.sh'])]
elif sys.argv[2:] == ["--public-entry-only"]:
 commands=[('reentrant-public-eval',['cargo','test','--workspace','--locked','--test','clojure_eval','compiled_eval_is_reentrant_inside_actual_compiled_macro_functions','--','--nocapture','--test-threads=2']),('complete-public-controls',['cargo','test','--workspace','--locked','--test','clojure_eval','--','--nocapture','--test-threads=2'])]
else:assert len(sys.argv)==2,"Unknown installation gate selection"
save();code=0
for label,command in commands:
 start=time.time()
 with (out/(label+'.log')).open('w') as log:result=subprocess.run(command,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
 entry={'label':label,'command':command,'exit_code':result.returncode,'seconds':time.time()-start};record['commands'].append(entry);save();announce(entry)
 if result.returncode:code=result.returncode;break
record.update(complete=True,exit_code=code,inputs_unchanged=all((root/p).is_file() and sha(root/p)==h for p,h in inputs.items()));save();announce({k:v for k,v in record.items() if k not in ['inputs','commands']})
raise SystemExit(code)
