"""Record real diagnostic execution on the explicit uncommitted source overlay."""
from pathlib import Path
import hashlib,json,os,signal,subprocess,time
signal.signal(signal.SIGHUP,signal.SIG_IGN)
E=Path(__file__).resolve().parent
J=Path('/home/bobby/.codex-workspaces/worktrees/ios-cd7ccbc9-1c87-43fe-95e7-2b6b71168849/suss/target/probe/m12-case-budget-source')
out=E/'macro-flag-opcode-guard-after';out.mkdir(exist_ok=False)
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
paths=set(subprocess.check_output(['git','ls-files','-co','--exclude-standard'],cwd=J,text=True).splitlines())
tracked=set(subprocess.check_output(['git','ls-files'],cwd=J,text=True).splitlines())
paths={p for p in paths if p in tracked or not p.startswith('results/') or any('primary' in part for part in Path(p).parts) or p.endswith('/cases.json') or p.endswith('/reference.json')}
inputs={p:sha(J/p) for p in sorted(paths) if (J/p).is_file()}
head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=J,text=True).strip();assert head=='3c4a59e85d12773f1105c14c65b865fba8186a60'
overlay=subprocess.check_output(['git','diff','--binary'],cwd=J);assert overlay
(out/'overlay.patch').write_bytes(overlay)
r={'scope':os.environ['SUSS_DURABLE_SCOPE'],'recorder_pid':os.getpid(),'head':head,'source_overlay_sha256':hashlib.sha256(overlay).hexdigest(),'inputs':inputs,'complete':False,'commands':[],'qualification':'Working-overlay diagnostic only, not a clean-head focused/full success. Current generic-core fix already passed all3 diagnostic commands. Test-only alignment explicitly checks macro mask before the unchanged inline-root/no-cast/dynamic-call structure. No production changes, no source/value/oracle policy edits. Same-head CI six-target failures and deliberately interrupted full101 preserved separately. Original primary/baseline/count/skip/opaque/allowances unchanged.'}
assert r['scope'] in Path('/proc/self/cgroup').read_text()
commands=[('complete-dynamic-ir-and-opcode-guards',['cargo','test','--locked','-p','suss-compile','--test','portable_dynamic_bindings','--','--nocapture','--test-threads=2'])]
env=os.environ|{'CARGO_TARGET_DIR':str(J.parent/'m12-some-fn-target'),'CARGO_BUILD_JOBS':'1','RUSTUP_TOOLCHAIN':'stable','SUSS_WASM_OPT':str(J.parent/'m11-binaryen/binaryen-version_133/bin/wasm-opt'),'BINARYEN_CORES':'1'}
env['PATH']=str(Path(env['SUSS_WASM_OPT']).parent)+os.pathsep+env['PATH']
r['runner_sha256']=sha(Path(__file__));r['binaryen']={str(Path(env['SUSS_WASM_OPT']).with_name(n)):sha(Path(env['SUSS_WASM_OPT']).with_name(n)) for n in ['wasm-opt','wasm-merge']}
def save():(out/'commands.json').write_text(json.dumps(r,indent=2)+'\n')
for name,command in commands:
    row={'name':name,'command':command,'complete':False};r['commands'].append(row);save();start=time.time()
    with (out/(name+'.log')).open('w') as log:result=subprocess.run(command,cwd=J,env=env,stdout=log,stderr=subprocess.STDOUT)
    row.update(complete=True,exit_code=result.returncode,seconds=time.time()-start);save();print(row,flush=True)
    if result.returncode:break
r.update(complete=True,head_unchanged=subprocess.check_output(['git','rev-parse','HEAD'],cwd=J,text=True).strip()==head,inputs_unchanged=all((J/p).is_file() and sha(J/p)==h for p,h in inputs.items()),overlay_unchanged=subprocess.check_output(['git','diff','--binary'],cwd=J)==overlay,binaryen_unchanged=all(sha(Path(p))==h for p,h in r['binaryen'].items()));save()
print({k:v for k,v in r.items() if k not in ['inputs','binaryen']},flush=True)
raise SystemExit(next((c['exit_code'] for c in r['commands'] if c.get('exit_code')),0) or (0 if all(r[k] for k in ['head_unchanged','inputs_unchanged','overlay_unchanged','binaryen_unchanged']) else 1))
