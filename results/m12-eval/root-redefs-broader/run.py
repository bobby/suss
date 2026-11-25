from pathlib import Path
import hashlib,json,os,subprocess,time
base=Path(__file__).resolve().parent;root=Path('/home/bobby/.codex-workspaces/worktrees/ios-cd7ccbc9-1c87-43fe-95e7-2b6b71168849/suss/target/probe/m12-case-budget-source');target=root.parent/'m12-some-fn-target';out=root/'results/m12-eval/root-redefs-broader';out.mkdir(exist_ok=False)
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
paths=set(subprocess.check_output(['git','ls-files','-co','--exclude-standard'],cwd=root,text=True).splitlines());tracked=set(subprocess.check_output(['git','ls-files'],cwd=root,text=True).splitlines());paths={p for p in paths if p in tracked or not p.startswith('results/') or any('primary' in part for part in Path(p).parts) or p.endswith('/cases.json') or p.endswith('/reference.json')}
inputs={p:sha(root/p) for p in sorted(paths) if (root/p).is_file()};head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
r={'scope':os.environ['SUSS_DURABLE_SCOPE'],'recorder_pid':os.getpid(),'head':head,'inputs':inputs,'complete':False,'commands':[],'qualification':'Fresh committedc9eecf80df791baba8fc51c19cc7d6e1ea136179 root redefinition versus thread-binding correction. All318focused tests0, protected source/primary/baseline conservation0, twelvefreshprimarycontrols/bothphases/actualmacros plusrootleaseforkGC/asyncawaitreturncancel/fuelinstallpop. Historical50full intentionally101foractualwith-redefswrongvalues andobsoleteCIcancelled; firstIRkindfocused101preserved. Freshbootstrap/native/strictthenexacttwo-threadfull. Source/head frozenuntilactualterminal; innerCargoresultpropagated. Independentreview/finalCI separate.'+head+' combined binding initializer-order/root-definition/pop correction. Current frozen focused cohort and retained before-value comparators are conserved by verify-root-redefs-checkpoint.py. Actual predecessorfull intentionally101forgenuineeffectbug; no historicalfull0 qualifies thissource. Fresh bootstrap/native/conservation then exactfullworkspace. Source/HEAD frozen untilactualterminal; innerCargo statuschecked/propagated. Originalprimary/baseline/count/skip/opaque rules unchanged. Independent review and final-headCI separate.'};assert r['scope'] in Path('/proc/self/cgroup').read_text()
def save():(out/'commands.json').write_text(json.dumps(r,indent=2)+'\n')
commands=[('focused-source-and-primary-conservation', ['python3', 'results/m12-eval/verify-root-redefs-checkpoint.py']), ('source-provenance', ['python3', 'results/m12-eval/verify-live-vars.py']), ('original-primary-provenance', ['python3', 'results/m12-eval/verify-primary.py']), ('core-import', ['python3', 'scripts/core_import.py', '--check']), ('sequence-provenance', ['python3', 'scripts/sequence_provenance.py']), ('comparison-contract', ['python3', 'scripts/test_clojure_test_suite.py']), ('optimized-random-effect-guard', ['cargo', 'test', '--workspace', '--locked', '--test', 'portable_random', 'optimized_selected_random_import_preserves_draws', '--', '--include-ignored', '--nocapture', '--test-threads=2']), ('java-node-free-bootstrap', ['sh', 'scripts/verify-bootstrap.sh']), ('fresh-complete-native-suite', ['python3', 'results/m12-reader-followup/run-suite.py', '/home/bobby/.codex-workspaces/worktrees/ios-cd7ccbc9-1c87-43fe-95e7-2b6b71168849/suss/target/probe/m12-case-budget-source/results/m12-eval/root-redefs-broader/complete-capture']), ('strict-current-complete-capture', ['python3', 'results/m12-eval/review-current-capture.py', '/home/bobby/.codex-workspaces/worktrees/ios-cd7ccbc9-1c87-43fe-95e7-2b6b71168849/suss/target/probe/m12-case-budget-source/results/m12-eval/root-redefs-broader/complete-capture']), ('exact-workspace-baseline', ['python3', 'results/m12-eval/context-fuel-accounting-focused/checked-workspace.py', '/home/bobby/.codex-workspaces/worktrees/ios-cd7ccbc9-1c87-43fe-95e7-2b6b71168849/suss/target/probe/m12-case-budget-source/results/m12-eval/full-workspace-after-root-redefs'])]
env=os.environ|{'CARGO_TARGET_DIR':str(target),'CARGO_BUILD_JOBS':'1','RUSTUP_TOOLCHAIN':'stable','SUSS_NUMERIC_BODY_CAPTURE':str(out/'capture'),'SUSS_SUITE_TARGET':str(target),'SUSS_WASM_OPT':str(root.parent/'m11-binaryen/binaryen-version_133/bin/wasm-opt'),'BINARYEN_CORES':'1'}
env['SUSS_RANDOM_TARGET']=str(target)
env['PATH']=str(Path(env['SUSS_WASM_OPT']).parent)+os.pathsep+env['PATH']
assert head=='c9eecf80df791baba8fc51c19cc7d6e1ea136179'
subprocess.run(['git','diff','--quiet'],cwd=root,check=True)
subprocess.run(['git','diff','--cached','--quiet'],cwd=root,check=True)
assert not (root/'results/m12-eval/full-workspace-after-root-redefs/command.json').exists()
r['runner_sha256']=sha(Path(__file__))
r['binaryen']={str(Path(env['SUSS_WASM_OPT']).with_name(name)):sha(Path(env['SUSS_WASM_OPT']).with_name(name)) for name in ['wasm-opt','wasm-merge']}
for name,command in commands:
 row={'name':name,'command':command,'complete':False};r['commands'].append(row);save();start=time.time()
 with (out/(name+'.log')).open('w') as log:p=subprocess.run(command,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
 row.update(complete=True,exit_code=p.returncode,seconds=time.time()-start);save();print(row,flush=True)
 if p.returncode:break
workspace=root/'results/m12-eval/full-workspace-after-root-redefs/command.json'
if workspace.exists():
 proof=json.loads(workspace.read_bytes())
 r['actual_workspace_exit_code']=proof.get('exit_code')
 r['required_workspace_passed']=bool(proof.get('complete') and proof.get('exit_code')==0 and proof.get('inputs_unchanged') and proof.get('head_unchanged') and proof.get('head')==head)
else:r['required_workspace_passed']=False
r['binaryen_unchanged']=all(sha(Path(p))==h for p,h in r['binaryen'].items())
r.update(complete=True,head_unchanged=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()==head,inputs_unchanged=all((root/p).is_file() and sha(root/p)==h for p,h in inputs.items()));save();print({k:v for k,v in r.items() if k!='inputs'},flush=True)

actual=next((row['exit_code'] for row in r['commands'] if row.get('exit_code')),0)
if actual==0 and not r['required_workspace_passed']:actual=r.get('actual_workspace_exit_code') or 1
raise SystemExit(actual)
