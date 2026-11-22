from pathlib import Path
import subprocess,os,json,time
root=Path.cwd();p=root/'results/m12-vector-nth-oracle';outer=root.parents[2]
env=os.environ|{'RUSTUP_TOOLCHAIN':'stable','CARGO_BUILD_JOBS':'2','CARGO_TARGET_DIR':str(outer/'target/probe/m12-aggregate-target'),'SUSS_BUILD_DIR':str(outer/'target/suss'),'XDG_RUNTIME_DIR':'/run/user/1000'}
binpath=outer/'target/probe/m11-binaryen/binaryen-version_133/bin';env['SUSS_WASM_OPT']=str(binpath/'wasm-opt');env['PATH']=str(binpath)+':'+env['PATH'];env['TMPDIR']=str(root/'target/probe/tmp')
targets=['clojure_nonescaping_closures','captured_fn_calls','apply_fixed_arity','portable_closure_receivers','portable_function_parameters','compiled_macro_function_scopes','compiled_macro_declaration_functions','portable_closures','portable_hir_resources','compiled_record_default_budget','repl_default_budgets','compiled_aot_compile_fuel','portable_async_execution','compiled_aot_async_command','clojure_raw_bits_and_casts','clojure_raw_numeric_core','clojure_primitive_method_types','clojure_numeric_extrema','clojure_local_numeric_hints','clojure_division_overloads','clojure_unboxed_locals','clojure_numeric_box_reuse','compiled_core_casts','compiled_macro_cumulative_fuel','compiled_primitive_params','portable_primitive_entries','portable_core_expression_dependencies','portable_unboxed_locals']
targets += ['clojure_nonescaping_vectors','portable_nonescaping_vectors','portable_calling_convention','portable_inline_predicates','compiled_vector_nth','compiled_macro_constructor_source_asts','portable_nominal','builtin_internals_sealed']
(root/'target/probe/tmp').mkdir(parents=True,exist_ok=True)
subprocess.run(['python3',str(p/'source-check.py')],check=True)
cmd=['cargo','+stable','test','--workspace','--locked']
for target in targets:cmd+=['--test',target]
cmd+=['--','--test-threads=2']
commands=[('focused',cmd,{}),('bootstrap',['sh','scripts/verify-bootstrap.sh'],{}),('binaryen',['cargo','+stable','test','--workspace','--locked','--test','compiled_optimize','--','--test-threads=2'],{}),('suite',['cargo','+stable','test','--workspace','--locked','--test','clojure_test_suite','clojure_test_suite_matches_reviewed_baseline','--','--exact','--test-threads=2'],{'SUSS_CLOJURE_TEST_SUITE_CAPTURE':str(root/'target/probe/m12-inline-nth-suite-capture')})]
records=[]
for name,cmd,extra in commands:
 start=time.time()
 with (p/(name+'.log')).open('w') as log:r=subprocess.run(['systemd-run','--user','--scope','-p','MemoryMax=4G','--',*cmd],env=env|extra,stdout=log,stderr=subprocess.STDOUT)
 records.append({'name':name,'command':cmd,'environment':extra,'exit_code':r.returncode,'seconds':time.time()-start});(p/'commands.json').write_text(json.dumps(records,indent=2)+'\n');print(name,r.returncode,flush=True)
 if r.returncode:raise SystemExit(r.returncode)

subprocess.run(['python3',str(p/'source-check.py')],check=True)
