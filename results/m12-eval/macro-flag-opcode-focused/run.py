from pathlib import Path
import hashlib,json,os,signal,subprocess,time
signal.signal(signal.SIGHUP,signal.SIG_IGN)
base=Path(__file__).resolve().parent
root=Path('/home/bobby/.codex-workspaces/worktrees/ios-cd7ccbc9-1c87-43fe-95e7-2b6b71168849/suss/target/probe/m12-case-budget-source')
target=root.parent/'m12-some-fn-target';out=root/'results/m12-eval/macro-flag-opcode-focused';out.mkdir(exist_ok=False)
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
paths=set(subprocess.check_output(['git','ls-files','-co','--exclude-standard'],cwd=root,text=True).splitlines());tracked=set(subprocess.check_output(['git','ls-files'],cwd=root,text=True).splitlines());paths={p for p in paths if p in tracked or not p.startswith('results/') or any('primary' in part for part in Path(p).parts) or p.endswith('/cases.json') or p.endswith('/reference.json')}
inputs={p:sha(root/p) for p in sorted(paths) if (root/p).is_file()};head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
r={'scope':os.environ['SUSS_DURABLE_SCOPE'],'recorder_pid':os.getpid(),'head':head,'inputs':inputs,'complete':False,'commands':[],'qualification':'Test-only fa0da alignment: production3c4a and all34 CI targets/fixtures unchanged, with actual176passes conserved by a strict separate verifier. Sole unselected opcode guard retains inline-root/no-cast/dynamic checks and now explicitly checks macro metadata mask; actualtarget3passes. Fresh6commands require retainedCIproof/currentcore/Rust/provenance and complete217eval59/ABI/dynamic/async/legacyVar/publication/AOT plus119host tests. Oldwhole101remains failure, not reused. Original primary/baseline/count/skip/opaque/allowances unchanged. Freshbroader/native/bootstrap/exactfull, independentxhighreview/finalheadCI remain required.'};assert r['scope'] in Path('/proc/self/cgroup').read_text()
def save():(out/'commands.json').write_text(json.dumps(r,indent=2)+'\n')
assert head=='fa0da7ec4ce9e37ef5493f1fc99d652b0f2d25be'
subprocess.run(['git','diff','--quiet'],cwd=root,check=True)
subprocess.run(['git','diff','--cached','--quiet'],cwd=root,check=True)
commands=[('retained-ci-and-production-conservation', ['python3', '/home/bobby/.codex-workspaces/cleanup/suss-20261123/eval-next-step/verify-fa0da-retained-ci-checkpoint.py']), ('core-import', ['python3', 'scripts/core_import.py', '--check']), ('current-source-rust-check', ['cargo', 'check', '-p', 'suss-compile', '--locked']), ('source-provenance', ['python3', 'results/m12-eval/verify-live-vars.py']), ('complete-eval-dynamic-and-async-targets', ['cargo', 'test', '--workspace', '--locked', '--test', 'runtime_abi', '--test', 'clojure_eval', '--test', 'compiled_macro_publication_cells', '--test', 'portable_dynamic_bindings', '--test', 'portable_closures', '--test', 'portable_resolution', '--test', 'runtime_dynamic_switch', '--test', 'runtime_async_scheduler', '--test', 'portable_async_analysis', '--test', 'clojure_raw_numeric_core', '--test', 'portable_symbol_conversion', '--', '--nocapture', '--test-threads=2']), ('host-library-regressions', ['cargo', 'test', '--workspace', '--locked', '--lib', 'portable_', '--', '--nocapture', '--test-threads=2'])]

env=os.environ|{'CARGO_TARGET_DIR':str(target),'CARGO_BUILD_JOBS':'1','RUSTUP_TOOLCHAIN':'stable','BINARYEN_CORES':'1'}
r['runner_sha256']=sha(Path(__file__))
r['external_tools']={str(p):sha(p) for p in [base/'verify-fa0da-retained-ci-checkpoint.py',base/'macro-flag-opcode-test-only-conservation.json']}
r['retained_ci_code_head']='3c4a59e85d12773f1105c14c65b865fba8186a60'
r['retained_ci_passed_tests']=176
r['observed_ci_targets']=['clojure_chunk_effects', 'clojure_uuid', 'compiled_macro_declaration_functions', 'compiled_macro_declaration_metadata', 'compiled_macro_form_source_metadata', 'compiled_macro_repeated_declarations', 'compiled_macro_syntax_quote', 'compiled_native_entrypoints', 'compiled_source_macros', 'core_sequence_interfaces', 'core_sorted_interfaces', 'portable_apply_sequences', 'portable_bitwise_hash', 'portable_caching_hash', 'portable_callable_metadata', 'portable_case', 'portable_collection_hash', 'portable_collection_literals', 'portable_default_hash', 'portable_forward_declarations', 'portable_identity_hash', 'portable_indexing', 'portable_iteration', 'portable_murmur_hash', 'portable_numeric_hash', 'portable_object_methods', 'portable_public_hash', 'portable_reduction', 'portable_seqable', 'portable_sequences', 'portable_source_apply', 'portable_string_hash', 'portable_type_method_scopes', 'portable_vector_trie']
r['ci_snapshot']={'path':str(base/'ci-0e-observed-failures-for-3c4a.json'),'sha256':sha(base/'ci-0e-observed-failures-for-3c4a.json')}
env['SUSS_WASM_OPT']=str(root.parent/'m11-binaryen/binaryen-version_133/bin/wasm-opt')
r['binaryen']={str(root.parent/'m11-binaryen/binaryen-version_133/bin'/name):sha(root.parent/'m11-binaryen/binaryen-version_133/bin'/name) for name in ['wasm-opt','wasm-merge']}
env['PATH']=str(root.parent/'m11-binaryen/binaryen-version_133/bin')+os.pathsep+env['PATH']
for name,command in commands:
 row={'name':name,'command':command,'complete':False};r['commands'].append(row);save();start=time.time()
 with (out/(name+'.log')).open('w') as log:p=subprocess.run(command,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
 row.update(complete=True,exit_code=p.returncode,seconds=time.time()-start);save();print(row,flush=True)
 if p.returncode:break
r['external_tools_unchanged']=all(sha(Path(p))==h for p,h in r['external_tools'].items())
r['ci_snapshot_unchanged']=sha(Path(r['ci_snapshot']['path']))==r['ci_snapshot']['sha256']
r.update(complete=True,head_unchanged=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()==head,inputs_unchanged=all((root/p).is_file() and sha(root/p)==h for p,h in inputs.items()),binaryen_unchanged=all(sha(Path(p))==h for p,h in r['binaryen'].items()));save();print({k:v for k,v in r.items() if k!='inputs'},flush=True)
raise SystemExit(next((x['exit_code'] for x in r['commands'] if x.get('exit_code')),0))
