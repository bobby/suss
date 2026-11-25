from pathlib import Path
import hashlib,json,os,signal,subprocess,time
signal.signal(signal.SIGHUP,signal.SIG_IGN)
base=Path(__file__).resolve().parent
root=Path('/home/bobby/.codex-workspaces/worktrees/ios-cd7ccbc9-1c87-43fe-95e7-2b6b71168849/suss/target/probe/m12-case-budget-source')
target=root.parent/'m12-some-fn-target';out=root/'results/m12-eval/with-redefs-root-focused';out.mkdir(exist_ok=False)
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
paths=set(subprocess.check_output(['git','ls-files','-co','--exclude-standard'],cwd=root,text=True).splitlines());tracked=set(subprocess.check_output(['git','ls-files'],cwd=root,text=True).splitlines());paths={p for p in paths if p in tracked or not p.startswith('results/') or any('primary' in part for part in Path(p).parts) or p.endswith('/cases.json') or p.endswith('/reference.json')}
inputs={p:sha(root/p) for p in sorted(paths) if (root/p).is_file()};head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
r={'diagnostic_storage':'GC-owned private string; language Error created and thrown on actual binding read, no per-snapshot host callback', 'scope':os.environ['SUSS_DURABLE_SCOPE'],'recorder_pid':os.getpid(),'head':head,'inputs':inputs,'complete':False,'commands':[],'qualification':'Separate with-redefs real-root leases from thread-binding frames in Sync/Async HIR/IR. Private GC-owned wrappers record real root and oldboundbit; selected thread reads ignore them; complete validation precedes writes, lease roots before root mutation, common pop restores only lease roots, forks omit globally-owned leases. Existing binding initializer/rootdef fixes retained. Fresh12primary controls bothphases/actualmacros, fork/rootownership GC, realpendingawait return/cancel, fullfuelinstall/pop sweep plusallpriorguards. Currenttarget no baseline/primary/count/skip/opaque changes. Completebroader/full/review/finalCI separate.'};assert r['scope'] in Path('/proc/self/cgroup').read_text()
def save():(out/'commands.json').write_text(json.dumps(r,indent=2)+'\n')
commands=[
 ('source-provenance',['python3','results/m12-eval/verify-live-vars.py']),
 ('complete-eval-dynamic-and-async-targets',['cargo','test','--workspace','--locked','--test','runtime_abi','--test','clojure_eval','--test','compiled_macro_publication_cells','--test','portable_dynamic_bindings','--test','portable_closures','--test','portable_resolution','--test','runtime_dynamic_switch','--test','runtime_async_scheduler','--test','portable_async_analysis','--test','clojure_raw_numeric_core','--','--nocapture','--test-threads=2']),
 ('host-library-regressions',['cargo','test','--workspace','--locked','--lib','portable_','--','--nocapture','--test-threads=2'])]

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
