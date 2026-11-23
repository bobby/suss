"""Source/fixture-frozen character, numeric-overload and original-budget gates."""
from pathlib import Path
import hashlib,json,os,subprocess,sys,time
ROOT=Path(__file__).resolve().parents[2];BASE=Path(__file__).resolve().parent;OUT=BASE/sys.argv[1];OUT.mkdir(exist_ok=True);assert not (OUT/'commands.json').exists()
paths=subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z'],cwd=ROOT).decode().split('\0')
inputs={p:hashlib.sha256((ROOT/p).read_bytes()).hexdigest() for p in paths if p and not p.startswith('results/') and (ROOT/p).is_file()}
for name in ['primary/cases.json','effects/cases.json','float-primary/cases.json','object-float-probe/cases.json']:
 p=BASE/name;inputs[str(p.relative_to(ROOT))]=hashlib.sha256(p.read_bytes()).hexdigest()
p=ROOT/'clojurescript/src/main/cljs/cljs/core.cljs';inputs[str(p.relative_to(ROOT))]=hashlib.sha256(p.read_bytes()).hexdigest()
patch=subprocess.check_output(['git','diff','--binary','HEAD','--','crates'],cwd=ROOT);(OUT/'source-diff.patch.gz').write_bytes(__import__('gzip').compress(patch,mtime=0))
commands=[
 ('character-and-phase-observers',['cargo','test','--workspace','--locked','--no-fail-fast','--test','clojure_char','--test','session_observation_hook','--','--test-threads=2']),
 ('original-numeric-casts',['cargo','test','--workspace','--locked','--no-fail-fast','--test','clojure_raw_bits_and_casts','--test','clojure_local_numeric_hints','--test','clojure_unchecked_ratios','--test','clojure_decimal_conversions','--test','clojure_primitive_method_types','--','--test-threads=2']),
 ('namespace-and-source-tooling',['cargo','test','--workspace','--locked','--no-fail-fast','--test','analyzer_edn','--test','inline_module_macros','--test','compiled_native_entrypoints','--test','compiled_macro_form_source_metadata','--test','compiled_macro_syntax_quote','--','--test-threads=2']),
 ('original-record-and-size-guards',['cargo','test','--workspace','--locked','--no-fail-fast','--test','compiled_record_default_budget','--test','runtime_namespaces','--test','clojure_calling_convention','--','--test-threads=2']),
 ('actual-link-failure-rollback',['cargo','test','-p','suss-compile','--locked','--lib','namespace_local_macros_roll_back_on_real_link_failure','--','--test-threads=2'])]
env=os.environ|{'CARGO_TARGET_DIR':str(ROOT.parent/'m12-case-followup-target'),'CARGO_BUILD_JOBS':'1','RUSTUP_TOOLCHAIN':'stable','XDG_RUNTIME_DIR':'/run/user/1000'}
record={'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'source_diff_sha256':hashlib.sha256(patch).hexdigest(),'inputs':inputs,'planned_commands':commands,'commands':[],'complete':False}
def save():(OUT/'commands.json').write_text(json.dumps(record,indent=2)+'\n')
save();failed=False
for label,command in commands:
 start=time.time()
 with (OUT/(label+'.log')).open('w') as log:r=subprocess.run(['systemd-run','--user','--scope','-p','MemoryMax=3G','--',*command],cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT)
 item={'label':label,'command':command,'exit_code':r.returncode,'seconds':time.time()-start,'inputs_unchanged':all((ROOT/p).is_file() and hashlib.sha256((ROOT/p).read_bytes()).hexdigest()==h for p,h in inputs.items())};record['commands'].append(item);save();print(item,flush=True)
 if r.returncode or not item['inputs_unchanged']:failed=True;break
record.update(complete=True,exit_code=int(failed),unexecuted_commands=[label for label,_ in commands[len(record['commands']):]]);save();raise SystemExit(int(failed))
