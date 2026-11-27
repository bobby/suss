from pathlib import Path
import hashlib,json,os,signal,subprocess,time
signal.signal(signal.SIGHUP,signal.SIG_IGN)
E=Path(__file__).resolve().parent;J=Path('/home/bobby/.codex-workspaces/worktrees/ios-cd7ccbc9-1c87-43fe-95e7-2b6b71168849/suss/target/probe/m12-case-budget-source');out=J/'results/m12-eval/portable-reference-workspace-gates';out.mkdir(exist_ok=False);full=J/'results/m12-eval/full-workspace-after-portable-reference-alignment';head='133b7024762d5006d4c784416e66ecef5a73f6b8';sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest();assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=J,text=True).strip()==head
for args in [['diff','--quiet'],['diff','--cached','--quiet']]:subprocess.run(['git',*args],cwd=J,check=True)
r={'head':head,'scope':os.environ['SUSS_DURABLE_SCOPE'],'pid':os.getpid(),'complete':False,'phase':'source-conservation-before-exact-workspace','commands':[],'runner_sha256':sha(Path(__file__)),'qualification':'Two test-only CI followups independently execute complete22portable and11module targets. Production318 and10broader bootstrap/native/strict passes conserved; C9full101 explicitly retained. This run must obtain actual final-head exactworkspace0. Source and HEAD frozen until actualterminal. Review/CI separate.'};assert r['scope'] in Path('/proc/self/cgroup').read_text()
bin=J.parent/'m11-binaryen/binaryen-version_133/bin';target=J.parent/'m12-some-fn-target';env=os.environ|{'CARGO_TARGET_DIR':str(target),'SUSS_RANDOM_TARGET':str(target),'SUSS_WASM_OPT':str(bin/'wasm-opt'),'CARGO_BUILD_JOBS':'1','BINARYEN_CORES':'1','RUSTUP_TOOLCHAIN':'stable'};env['PATH']=str(bin)+os.pathsep+env['PATH'];r['binaryen']={str(bin/n):sha(bin/n) for n in ['wasm-opt','wasm-merge']}
def save():(out/'commands.json').write_text(json.dumps(r,indent=2)+'\n')
commands=[('current-source-conservation',['python3','results/m12-eval/verify-portable-reference-checkpoint.py']),('source-provenance',['python3','results/m12-eval/verify-live-vars.py']),('original-primary',['python3','results/m12-eval/verify-primary.py']),('exact-workspace',['python3','results/m12-eval/context-fuel-accounting-focused/checked-workspace.py',str(full)])]
actual=0
for name,command in commands:
 row={'name':name,'command':command,'complete':False};r['commands'].append(row);r['phase']=name;save();start=time.time()
 with (out/(name+'.log')).open('w') as log:p=subprocess.run(command,cwd=J,env=env,stdout=log,stderr=subprocess.STDOUT)
 row.update(complete=True,exit_code=p.returncode,seconds=time.time()-start);save();print(row,flush=True)
 if p.returncode:actual=p.returncode;break
if (full/'command.json').exists():
 w=json.loads((full/'command.json').read_bytes());r['actual_workspace_exit']=w.get('exit_code');r['required_workspace_passed']=bool(w.get('complete') and w.get('exit_code')==0 and w.get('head')==head and w.get('inputs_unchanged') and w.get('head_unchanged'))
else:r['required_workspace_passed']=False
r.update(complete=True,phase='actual-terminal',exit_code=actual,head_unchanged=subprocess.check_output(['git','rev-parse','HEAD'],cwd=J,text=True).strip()==head,binaryen_unchanged=all(sha(Path(p))==h for p,h in r['binaryen'].items()));save();print(r,flush=True);raise SystemExit(actual or (0 if r['required_workspace_passed'] else 1))
