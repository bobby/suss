"""Focused before/after review regressions, then original source/cache guards."""
from pathlib import Path
import hashlib,json,os,subprocess,sys,time
ROOT=Path(__file__).resolve().parents[2];BASE=Path(__file__).resolve().parent
OUT=BASE/sys.argv[1];OUT.mkdir(exist_ok=True);assert not (OUT/'commands.json').exists()
commands=[
 ('partial-import-recovery',['cargo','test','--workspace','--locked','--test','inline_module_macros','partial_macro_import_failure_recovers_source_identities_and_emission_reuse','--','--nocapture','--test-threads=2']),
 ('uuid-runtime-syntax-quote',['cargo','test','--workspace','--locked','--test','clojure_uuid','syntax_quote','--','--nocapture','--test-threads=2']),
 ('source-and-cache-regressions',['cargo','test','--workspace','--locked','--no-fail-fast','--test','inline_module_macros','--test','compiled_module_cache','--test','compiled_source_artifact_cache','--test','compiled_macro_reload','--test','compiled_macro_source_positions','--test','compiled_macro_syntax_quote','--test','clojure_uuid','--test','compiled_bootstrap','--','--test-threads=2']),
 ('original-record-and-artifact-guards',['cargo','test','--workspace','--locked','--no-fail-fast','--test','compiled_record_default_budget','--test','runtime_namespaces','--test','portable_calling_convention','--','--test-threads=2']),
]
paths=subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z'],cwd=ROOT).decode().split('\0')
inputs={p:hashlib.sha256((ROOT/p).read_bytes()).hexdigest() for p in paths if p and not p.startswith('results/') and (ROOT/p).is_file()}
for name in ['reader-cases.json','uuid-parser-cases.edn','uuid-hex-digit-jvm.bin','uuid-runtime-cases.json','uuid-boundary-cases.json','uuid-case-runtime-cases.json','uuid-case-expansion-cases.json','uuid-runtime-syntax-quote-primary/cases.json']:
 p=BASE/name;inputs[str(p.relative_to(ROOT))]=hashlib.sha256(p.read_bytes()).hexdigest()
core=ROOT/'clojurescript/src/main/cljs/cljs/core.cljs';inputs[str(core.relative_to(ROOT))]=hashlib.sha256(core.read_bytes()).hexdigest()
env=os.environ|{'CARGO_TARGET_DIR':str(ROOT.parent/'m12-review-repair-target'),'CARGO_BUILD_JOBS':'1','RUSTUP_TOOLCHAIN':'stable','XDG_RUNTIME_DIR':'/run/user/1000'}
record={'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'inputs':inputs,'target':env['CARGO_TARGET_DIR'],'planned_commands':commands,'commands':[],'complete':False}
def save():(OUT/'commands.json').write_text(json.dumps(record,indent=2)+'\n')
save();failed=False
for label,command in commands:
 started=time.time()
 with (OUT/(label+'.log')).open('w') as log:
  result=subprocess.run(['systemd-run','--user','--scope','-p','MemoryMax=3G','--',*command],cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT)
 unchanged=all(hashlib.sha256((ROOT/p).read_bytes()).hexdigest()==h for p,h in inputs.items())
 item={'label':label,'command':command,'exit_code':result.returncode,'seconds':time.time()-started,'inputs_unchanged':unchanged};record['commands'].append(item);save()
 print({k:v for k,v in item.items() if k!='command'},flush=True)
 if result.returncode or not unchanged:failed=True;break
record.update(complete=True,exit_code=int(failed),unexecuted_commands=[name for name,_ in commands[len(record['commands']):]])
save();raise SystemExit(int(failed))
