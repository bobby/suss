from pathlib import Path
import hashlib,json,os,subprocess,sys,time
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
label=sys.argv[1]
command=['cargo','test','--workspace','--locked','--no-fail-fast','--test','portable_calling_convention','--test','portable_comparator_foundations','--test','read_string','--test','decimal_values','--test','clojure_decimal_values','--','--test-threads=2']
env=os.environ|{'XDG_RUNTIME_DIR':'/run/user/1000','CARGO_BUILD_JOBS':'2','RUSTUP_TOOLCHAIN':'stable','CARGO_TARGET_DIR':str(ROOT.parent/'m12-decimal-target')}
paths=subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z'],cwd=ROOT).decode().split('\0')
inputs={p:hashlib.sha256((ROOT/p).read_bytes()).hexdigest() for p in paths if p and not p.startswith('results/') and (ROOT/p).is_file()}
fixtures={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in OUT.iterdir() if p.name in ['cases.json','carrier-cases.json']}
record={'fixtures':fixtures,'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'inputs':inputs,'cases_sha256':hashlib.sha256((OUT/'cases.json').read_bytes()).hexdigest(),'command':command,'complete':False}
def save():(OUT/(label+'.json')).write_text(json.dumps(record,indent=2)+'\n')
save();start=time.time()
with (OUT/(label+'.log')).open('w') as out:
 p=subprocess.run(['systemd-run','--user','--scope','-p','MemoryMax=4G','--',*command],cwd=ROOT,env=env,stdout=out,stderr=subprocess.STDOUT)
record.update(exit_code=p.returncode,seconds=time.time()-start,complete=True,inputs_unchanged=all(hashlib.sha256((ROOT/f).read_bytes()).hexdigest()==v for f,v in inputs.items()));save();print({k:v for k,v in record.items() if k!='inputs'},flush=True)
