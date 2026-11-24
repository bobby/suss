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
tracked=set(subprocess.check_output(['git','ls-files','--cached','-z'],cwd=root).decode().split('\0'))
inputs={p:sha(root/p) for p in paths if p and (p in tracked or not p.startswith('results/')) and (root/p).is_file()}
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
elif sys.argv[2:] == ["--broader-eval-only"]:
 env['SUSS_SUITE_TARGET']=env['CARGO_TARGET_DIR']
 env['SUSS_WASM_OPT']='/home/bobby/.codex-workspaces/worktrees/ios-cd7ccbc9-1c87-43fe-95e7-2b6b71168849/suss/target/probe/m11-binaryen/binaryen-version_133/bin/wasm-opt'
 env['PATH']=str(Path(env['SUSS_WASM_OPT']).parent)+os.pathsep+env['PATH'];env['BINARYEN_CORES']='1'
 targets=['runtime_evaluation','runtime_abi','portable_modules','portable_phase_modules','inline_module_macros','runtime_namespaces','namespace_session','compiled_namespace_acceptance','compiled_aot_namespace_command','compiled_record_default_budget','repl_default_budgets','compiled_source_artifact_cache','compiled_artifact_identities','public_expression_artifacts','uncached_expression_artifacts','portable_record_identity','compiled_call_sites','clojure_bigints','clojure_ratios','clojure_decimal_arithmetic','clojure_decimal_values','clojure_decimal_constructors','clojure_decimal_conversions']
 commands=[('complete-public-controls',['cargo','test','--workspace','--locked','--test','clojure_eval','--','--nocapture','--test-threads=2']),('existing-host-regressions',['cargo','test','--workspace','--locked','--lib','portable_','--','--nocapture','--test-threads=2']),('numeric-module-artifact-budget-regressions',['cargo','test','--workspace','--locked',*[item for target in targets for item in ['--test',target]],'--','--nocapture','--test-threads=2']),('optimized-random-effect-guard',['cargo','test','--workspace','--locked','--test','portable_random','optimized_selected_random_import_preserves_draws','--','--include-ignored','--nocapture','--test-threads=2']),('java-node-free-bootstrap',['sh','scripts/verify-bootstrap.sh']),('complete-suite',['python3','results/m12-reader-followup/run-suite.py',str(out/'complete-capture')]),('review-complete-capture',['python3','results/m12-eval/review-complete-capture.py',str(out/'complete-capture')])]
 for name in ['wasm-opt','wasm-merge']:
  tool=Path(env['SUSS_WASM_OPT']).with_name(name);record.setdefault('binaryen',{})[str(tool)]=sha(tool)
elif sys.argv[2:] == ["--capability-budget-only"]:
 commands=[('complete-public-controls-capabilities-budgets',['cargo','test','--workspace','--locked','--test','clojure_eval','--','--nocapture','--test-threads=2']),('complete-original-eval-body',['cargo','test','--workspace','--locked','--test','clojure_test_suite','complete_eval_body_keeps_exact_numeric_fields_as_host_matches','--','--nocapture','--test-threads=2'])]
elif sys.argv[2:] == ["--namespace-only"]:
 commands=[('namespace-primary',['cargo','test','--workspace','--locked','--test','clojure_eval','eval_namespace_changes_follow_real_caller_binding_frames','--','--nocapture','--test-threads=2']),('complete-public-controls',['cargo','test','--workspace','--locked','--test','clojure_eval','--','--nocapture','--test-threads=2']),('complete-original-eval-body',['cargo','test','--workspace','--locked','--test','clojure_test_suite','complete_eval_body_keeps_exact_numeric_fields_as_host_matches','--','--nocapture','--test-threads=2'])]
elif sys.argv[2:] == ["--do-boundary-only"]:
 commands=[('top-level-do-primary',['cargo','test','--workspace','--locked','--test','clojure_eval','eval_preserves_primary_top_level_do_compilation_and_effect_boundaries','--','--nocapture','--test-threads=2']),('complete-public-controls',['cargo','test','--workspace','--locked','--test','clojure_eval','--','--nocapture','--test-threads=2']),('complete-original-eval-body',['cargo','test','--workspace','--locked','--test','clojure_test_suite','complete_eval_body_keeps_exact_numeric_fields_as_host_matches','--','--nocapture','--test-threads=2'])]
elif sys.argv[2:] == ["--live-vars-only"]:
 commands=[('live-vars-primary',['cargo','test','--workspace','--locked','--test','clojure_eval','vars_retain_real_live_cells_identity_metadata_and_callability','--','--nocapture','--test-threads=2']),('complete-original-eval-body',['cargo','test','--workspace','--locked','--test','clojure_test_suite','complete_eval_body_keeps_exact_numeric_fields_as_host_matches','--','--nocapture','--test-threads=2']),('complete-public-controls',['cargo','test','--workspace','--locked','--test','clojure_eval','--','--nocapture','--test-threads=2'])]
elif sys.argv[2:] == ["--public-entry-only"]:
 commands=[('reentrant-public-eval',['cargo','test','--workspace','--locked','--test','clojure_eval','compiled_eval_is_reentrant_inside_actual_compiled_macro_functions','--','--nocapture','--test-threads=2']),('complete-public-controls',['cargo','test','--workspace','--locked','--test','clojure_eval','--','--nocapture','--test-threads=2'])]
else:assert len(sys.argv)==2,"Unknown installation gate selection"
save();code=0
for label,command in commands:
 start=time.time()
 with (out/(label+'.log')).open('w') as log:result=subprocess.run(command,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
 entry={'label':label,'command':command,'exit_code':result.returncode,'seconds':time.time()-start};record['commands'].append(entry);save();announce(entry)
 if result.returncode:code=result.returncode;break
record.update(complete=True,exit_code=code,head_unchanged=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()==record['head'],inputs_unchanged=all((root/p).is_file() and sha(root/p)==h for p,h in inputs.items()));save();announce({k:v for k,v in record.items() if k not in ['inputs','commands']})
raise SystemExit(code)
