"""Bounded semantic, linker, frontend and existing-budget gates for inline macros."""
from pathlib import Path
import hashlib,json,os,subprocess,sys,time
ROOT=Path(__file__).resolve().parents[2];BASE=Path(__file__).resolve().parent;OUT=BASE/sys.argv[1];OUT.mkdir(exist_ok=True);assert not (OUT/'commands.json').exists()
paths=subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z'],cwd=ROOT).decode().split('\0')
inputs={p:hashlib.sha256((ROOT/p).read_bytes()).hexdigest() for p in paths if p and not p.startswith('results/') and (ROOT/p).is_file()}
for name in ['reader-cases.json','uuid-parser-cases.edn','uuid-hex-digit-jvm.bin','uuid-runtime-cases.json','uuid-boundary-cases.json','uuid-case-runtime-cases.json','uuid-case-expansion-cases.json']:
 p=BASE/name;inputs[str(p.relative_to(ROOT))]=hashlib.sha256(p.read_bytes()).hexdigest()
p=ROOT/'clojurescript/src/main/cljs/cljs/core.cljs';inputs[str(p.relative_to(ROOT))]=hashlib.sha256(p.read_bytes()).hexdigest()
patch=subprocess.check_output(['git','diff','--binary','HEAD','--','crates'],cwd=ROOT);(OUT/'source-diff.patch').write_bytes(patch)
commands=[('inline-and-native-frontends',['cargo','test','--workspace','--locked','--no-fail-fast','--test','inline_module_macros','--test','compiled_native_entrypoints','--test','compiled_macro_form_source_metadata','--test','compiled_macro_syntax_quote','--','--test-threads=2']),('actual-link-failure-rollback',['cargo','test','-p','suss-compile','--locked','--lib','namespace_local_macros_roll_back_on_real_link_failure','--','--nocapture','--test-threads=2']),('uuid-and-independent-case-values',['cargo','test','--workspace','--locked','--test','clojure_uuid','--','--test-threads=2']),('original-record-and-size-guards',['cargo','test','--workspace','--locked','--no-fail-fast','--test','compiled_record_default_budget','--test','runtime_namespaces','--','--test-threads=2'])]
env=os.environ|{'CARGO_TARGET_DIR':str(ROOT.parent/'m12-suite-platform-target'),'CARGO_BUILD_JOBS':'1','RUSTUP_TOOLCHAIN':'stable','XDG_RUNTIME_DIR':'/run/user/1000'}
record={'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'source_diff_sha256':hashlib.sha256(patch).hexdigest(),'inputs':inputs,'planned_commands':commands,'commands':[],'complete':False}
def save():(OUT/'commands.json').write_text(json.dumps(record,indent=2)+'\n')
save();failed=False
for label,command in commands:
 start=time.time()
 with (OUT/(label+'.log')).open('w') as log:r=subprocess.run(['systemd-run','--user','--scope','-p','MemoryMax=3G','--',*command],cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT)
 item={'label':label,'command':command,'exit_code':r.returncode,'seconds':time.time()-start,'inputs_unchanged':all((ROOT/p).is_file() and hashlib.sha256((ROOT/p).read_bytes()).hexdigest()==h for p,h in inputs.items())};record['commands'].append(item);save();print(item,flush=True)
 if r.returncode or not item['inputs_unchanged']:failed=True;break
record.update(complete=True,exit_code=int(failed),unexecuted_commands=[label for label,_ in commands[len(record['commands']):]]);save();raise SystemExit(int(failed))
