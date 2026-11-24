from pathlib import Path
import hashlib,json,os,subprocess,time
base=Path(__file__).resolve().parent;root=base/'m12-numeric-completion';target=base/'pattern-compiled-target';out=root/'results/m12-num-api/ci5-all-repairs-validation';out.mkdir(exist_ok=False)
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
paths=set(subprocess.check_output(['git','ls-files','-co','--exclude-standard'],cwd=root,text=True).splitlines());tracked=set(subprocess.check_output(['git','ls-files'],cwd=root,text=True).splitlines());paths={p for p in paths if p in tracked or not p.startswith('results/') or any('primary' in part for part in Path(p).parts) or p.endswith('/cases.json') or p.endswith('/reference.json')}
inputs={p:sha(root/p) for p in sorted(paths) if (root/p).is_file()};head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
r={'scope':os.environ['SUSS_DURABLE_SCOPE'],'recorder_pid':os.getpid(),'head':head,'inputs':inputs,'complete':False,'commands':[],'qualification':'Complete CI5 repair gates: all8 cumulative-macro-fuel tests, original complete raw AOT matrix, all22 portable pipeline tests including397 fixed-source observations and142 unchanged retargets. Two fresh Clojure numeric identity expectations; same100k budget and128 finite effectful calls with individual success/exhaustion/effect retention/recovery controls. Original fixture and all production1229 inputs unchanged; fresh actual native0/strict conservation followed by external exact26-body check. Final exact workspace/review/CI remain.'};assert r['scope'] in Path('/proc/self/cgroup').read_text()
def save():(out/'commands.json').write_text(json.dumps(r,indent=2)+'\n')
commands=[('complete-macro-fuel-and-raw-aot-targets', ['cargo', 'test', '-p', 'suss-cli', '--locked', '--offline', '--test', 'compiled_macro_cumulative_fuel', '--test', 'compiled_raw_value_uses', '--', '--nocapture', '--test-threads=2']), ('complete-portable-pipeline', ['cargo', 'test', '-p', 'suss-compile', '--locked', '--offline', '--test', 'portable_pipeline', '--', '--nocapture', '--test-threads=2']), ('core-import', ['python3', 'scripts/core_import.py', '--check']), ('eval-source-provenance', ['python3', 'results/m12-eval/verify-live-vars.py']), ('original-comparison-contract', ['python3', 'scripts/test_clojure_test_suite.py']), ('explicit-failure-ledger-review', ['python3', 'results/m12-num-api/review-expanded-failure-ledger.py']), ('fresh-complete-native-suite', ['python3', 'results/m12-reader-followup/run-suite.py', '/home/bobby/.codex-workspaces/cleanup/suss-20261123/eval-next-step/m12-numeric-completion/results/m12-num-api/ci5-all-repairs-validation/complete-capture']), ('strict-current-integrated-capture', ['python3', 'results/m12-num-api/review-integrated-capture.py', '/home/bobby/.codex-workspaces/cleanup/suss-20261123/eval-next-step/m12-numeric-completion/results/m12-num-api/ci5-all-repairs-validation/complete-capture'])]
env=os.environ|{'CARGO_TARGET_DIR':str(target),'CARGO_BUILD_JOBS':'1','RUSTUP_TOOLCHAIN':'stable','SUSS_NUMERIC_BODY_CAPTURE':str(out/'capture'),'SUSS_SUITE_TARGET':str(target),'SUSS_WASM_OPT':'/home/bobby/.codex-workspaces/worktrees/ios-cd7ccbc9-1c87-43fe-95e7-2b6b71168849/suss/target/probe/m11-binaryen/binaryen-version_133/bin/wasm-opt','BINARYEN_CORES':'1'}
old=json.loads((root/'results/m12-num-api/integrated-eval-review-and-ledger-validation/commands.json').read_text())
assert old['complete'] and old['inputs_unchanged'] and old['head_unchanged'] and all(row['exit_code']==0 for row in old['commands'])
production={p:h for p,h in old['inputs'].items() if '/src/' in p or p.startswith(('runtime/','scripts/','.cargo/','.github/')) or p in ['Cargo.toml','Cargo.lock'] or p.endswith('/build.rs')}
assert all((root/p).is_file() and sha(root/p)==h for p,h in production.items()),'production input changed'
r['production_inputs_unchanged_from_7b71']=True
r['production_input_count']=len(production)
env['PATH']=str(Path(env['SUSS_WASM_OPT']).parent)+os.pathsep+env['PATH']
r['runner_sha256']=sha(Path(__file__))
r['binaryen']={str(Path(env['SUSS_WASM_OPT']).with_name(name)):sha(Path(env['SUSS_WASM_OPT']).with_name(name)) for name in ['wasm-opt','wasm-merge']}
for name,command in commands:
 row={'name':name,'command':command,'complete':False};r['commands'].append(row);save();start=time.time()
 with (out/(name+'.log')).open('w') as log:p=subprocess.run(command,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
 row.update(complete=True,exit_code=p.returncode,seconds=time.time()-start);save();print(row,flush=True)
 if p.returncode:break
r['binaryen_unchanged']=all(sha(Path(p))==h for p,h in r['binaryen'].items())
r.update(complete=True,head_unchanged=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()==head,inputs_unchanged=all((root/p).is_file() and sha(root/p)==h for p,h in inputs.items()));save();print({k:v for k,v in r.items() if k!='inputs'},flush=True)

raise SystemExit(next((row["exit_code"] for row in r["commands"] if row.get("exit_code")),0))
