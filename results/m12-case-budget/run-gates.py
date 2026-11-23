"""Verify eager compiled-case construction with unchanged acceptance guards."""
from pathlib import Path
import hashlib,json,os,subprocess,sys,time
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent/sys.argv[1];OUT.mkdir(exist_ok=True);assert not (OUT/'commands.json').exists()
paths=subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z'],cwd=ROOT).decode().split('\0');inputs={p:hashlib.sha256((ROOT/p).read_bytes()).hexdigest() for p in paths if p and not p.startswith('results/') and (ROOT/p).is_file()}
for p in (ROOT/'results').rglob('*cases.json'):
 inputs[str(p.relative_to(ROOT))]=hashlib.sha256(p.read_bytes()).hexdigest()
p=ROOT/'results/m12-case-budget/legacy-case-primary.json';inputs[str(p.relative_to(ROOT))]=hashlib.sha256(p.read_bytes()).hexdigest()
commands=[
 ('legacy-case-primary',['cargo','test','--workspace','--locked','--test','portable_case','--','--test-threads=2']),
 ('analyzer-round-trips',['cargo','test','--workspace','--locked','--test','analyzer_edn','--test','analyzer_edn_core','--','--test-threads=2']),
 ('method-source-and-primitive-hints',['cargo','test','--workspace','--locked','--test','compiled_macro_function_scopes','--test','compiled_macro_source_environment','--test','compiled_native_entrypoints','--test','clojure_local_numeric_hints','--','--test-threads=2']),
 ('record-budget',['cargo','test','--workspace','--locked','--test','compiled_record_default_budget','--','--nocapture','--test-threads=2']),
 ('all-case-controls',['cargo','test','--workspace','--locked','--test','clojure_case_dispatch','--','--test-threads=2']),
 ('full-case-body',['cargo','test','--workspace','--locked','--test','clojure_test_suite','pinned_case_body_matches_all_primary_assertions','--','--exact','--test-threads=2']),
 ('independent-gc-and-privacy',['cargo','test','--workspace','--locked','--test','portable_value_decoder','--test','builtin_internals_sealed','--','--test-threads=2']),
 ('component-identities',['cargo','test','--workspace','--locked','--test','compiled_artifact_identities','--','--test-threads=2']),
 ('aot-size',['cargo','test','--workspace','--locked','--test','runtime_namespaces','aot_namespace_frame_size_is_bounded_per_form','--','--exact','--nocapture','--test-threads=2']),
 ('no-java-bootstrap',['sh','scripts/verify-bootstrap.sh'])]
env=os.environ|{'CARGO_TARGET_DIR':str(ROOT.parent/'m12-case-followup-target'),'CARGO_BUILD_JOBS':'1','RUSTUP_TOOLCHAIN':'stable','XDG_RUNTIME_DIR':'/run/user/1000'}
record={'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'inputs':inputs,'commands':[],'complete':False}
def save():(OUT/'commands.json').write_text(json.dumps(record,indent=2)+'\n')
save();failed=False
for label,cmd in commands:
 start=time.time()
 with (OUT/(label+'.log')).open('w') as log:r=subprocess.run(['systemd-run','--user','--scope','-p','MemoryMax=3G','--',*cmd],cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT)
 item={'label':label,'command':cmd,'exit_code':r.returncode,'seconds':time.time()-start,'inputs_unchanged':all((ROOT/p).is_file() and hashlib.sha256((ROOT/p).read_bytes()).hexdigest()==h for p,h in inputs.items())};record['commands'].append(item);save();print(item,flush=True)
 if r.returncode or not item['inputs_unchanged']:failed=True;break
record.update(complete=True,exit_code=int(failed));save();raise SystemExit(int(failed))
