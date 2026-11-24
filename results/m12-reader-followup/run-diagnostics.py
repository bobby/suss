"""Execute UUID integration controls with immutable-input qualification."""
from pathlib import Path
import hashlib,json,os,subprocess,sys,time
ROOT=Path(__file__).resolve().parents[2];BASE=Path(__file__).resolve().parent;OUT=BASE/sys.argv[1];OUT.mkdir(exist_ok=True);assert not (OUT/'commands.json').exists()
paths=subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z'],cwd=ROOT).decode().split('\0')
inputs={p:hashlib.sha256((ROOT/p).read_bytes()).hexdigest() for p in paths if p and not p.startswith('results/') and (ROOT/p).is_file()}
for name in ['reader-cases.json','uuid-parser-cases.edn','uuid-hex-digit-jvm.bin','uuid-runtime-cases.json','uuid-boundary-cases.json']:
 p=BASE/name;inputs[str(p.relative_to(ROOT))]=hashlib.sha256(p.read_bytes()).hexdigest()
commands=[('source-core',['cargo','test','-p','suss-cli','--locked','--test','clojure_uuid','ordinary_source_core_loads_uuid_readers_in_both_phases','--','--exact','--nocapture','--test-threads=2']),('record-profile',['env','SUSS_RAW_KEY_TRACE=1','cargo','test','--workspace','--locked','--test','compiled_record_default_budget','repeated_repl_records_fit_four_million_and_keep_membership_below_one_tenth_default','--','--exact','--nocapture','--test-threads=2'])]
env=os.environ|{'CARGO_TARGET_DIR':str(ROOT.parent/'m12-suite-platform-target'),'CARGO_BUILD_JOBS':'1','RUSTUP_TOOLCHAIN':'stable','XDG_RUNTIME_DIR':'/run/user/1000'}
record={'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'inputs':inputs,'commands':[],'complete':False}
def save():(OUT/'commands.json').write_text(json.dumps(record,indent=2)+'\n')
save();failed=False
for label,cmd in commands:
 start=time.time()
 with (OUT/(label+'.log')).open('w') as log:r=subprocess.run(['systemd-run','--user','--scope','-p','MemoryMax=3G','--',*cmd],cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT)
 item={'label':label,'command':cmd,'exit_code':r.returncode,'seconds':time.time()-start,'inputs_unchanged':all((ROOT/p).is_file() and hashlib.sha256((ROOT/p).read_bytes()).hexdigest()==h for p,h in inputs.items())};record['commands'].append(item);save();print(item,flush=True)
 if r.returncode or not item['inputs_unchanged']:failed=True
record.update(complete=True,exit_code=int(failed));save();raise SystemExit(int(failed))
