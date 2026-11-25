from pathlib import Path
import hashlib,json,os,signal,subprocess,time
signal.signal(signal.SIGHUP,signal.SIG_IGN)
base=Path(__file__).resolve().parent
root=Path('/home/bobby/.codex-workspaces/worktrees/ios-cd7ccbc9-1c87-43fe-95e7-2b6b71168849/suss/target/probe/m12-case-budget-source')
target=root.parent/'m12-some-fn-target';out=root/'results/m12-eval/lazy-math-context-frame-focused';out.mkdir(exist_ok=False)
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
paths=set(subprocess.check_output(['git','ls-files','-co','--exclude-standard'],cwd=root,text=True).splitlines());tracked=set(subprocess.check_output(['git','ls-files'],cwd=root,text=True).splitlines());paths={p for p in paths if p in tracked or not p.startswith('results/') or any('primary' in part for part in Path(p).parts) or p.endswith('/cases.json') or p.endswith('/reference.json')}
inputs={p:sha(root/p) for p in sorted(paths) if (root/p).is_file()};head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
r={'diagnostic_storage':'GC-owned private string; language Error created and thrown on actual binding read, no per-snapshot host callback', 'scope':os.environ['SUSS_DURABLE_SCOPE'],'recorder_pid':os.getpid(),'head':head,'inputs':inputs,'complete':False,'commands':[],'qualification':'Lazy validation candidate: real nominal invalid data preserved, unused opaque/lazy bindings inert, transport diagnostics deferred to binding reads; original nominal macro data boundary remains explicit. Twenty fresh Clojure controls, including nested overrides and exception payloads; private dynamic frame snapshots preserve deferred values without forcing them. Fresh Clojure controls in Runtime/Macro plus actual Macro execution. Actual initial14 controls passed separately before private-wrapper cache refinement. No issue closure/promotion claim. Broader capture/bootstrap/workspace/review/final-head CI remain.'};assert r['scope'] in Path('/proc/self/cgroup').read_text()
def save():(out/'commands.json').write_text(json.dumps(r,indent=2)+'\n')
commands=[('source-provenance',['python3','results/m12-eval/verify-live-vars.py']),('complete-public-eval-and-runtime-abi',['cargo','test','--workspace','--locked','--test','clojure_eval','--test','runtime_abi','--','--nocapture','--test-threads=2']),('borrowed-installation',['cargo','test','-p','suss-compile','--locked','--lib','portable_session::installation::tests','--','--nocapture','--test-threads=2'])]
env=os.environ|{'CARGO_TARGET_DIR':str(target),'CARGO_BUILD_JOBS':'1','RUSTUP_TOOLCHAIN':'stable','BINARYEN_CORES':'1'}
r['runner_sha256']=sha(Path(__file__))
r['binaryen']={str(root.parent/'m11-binaryen/binaryen-version_133/bin'/name):sha(root.parent/'m11-binaryen/binaryen-version_133/bin'/name) for name in ['wasm-opt','wasm-merge']}
env['PATH']=str(root.parent/'m11-binaryen/binaryen-version_133/bin')+os.pathsep+env['PATH']
for name,command in commands:
 row={'name':name,'command':command,'complete':False};r['commands'].append(row);save();start=time.time()
 with (out/(name+'.log')).open('w') as log:p=subprocess.run(command,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
 row.update(complete=True,exit_code=p.returncode,seconds=time.time()-start);save();print(row,flush=True)
 if p.returncode:break
r.update(complete=True,head_unchanged=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()==head,inputs_unchanged=all((root/p).is_file() and sha(root/p)==h for p,h in inputs.items()),binaryen_unchanged=all(sha(Path(p))==h for p,h in r['binaryen'].items()));save();print({k:v for k,v in r.items() if k!='inputs'},flush=True)
raise SystemExit(next((x['exit_code'] for x in r['commands'] if x.get('exit_code')),0))
