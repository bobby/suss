"""Run the compiled assert regression without changing primary/body/budgets."""
from pathlib import Path
import hashlib,json,os,subprocess,sys,time
ROOT=Path(__file__).resolve().parents[2];BASE=Path(__file__).resolve().parent;OUT=BASE/sys.argv[1]
OUT.mkdir(exist_ok=True);assert not (OUT/'command.json').exists()
command=['cargo','test','--workspace','--locked','--test','clojure_assert',*sys.argv[2:],'--','--test-threads=2']
paths=subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z'],cwd=ROOT).decode().split('\0')
inputs={p:hashlib.sha256((ROOT/p).read_bytes()).hexdigest() for p in paths if p and not p.startswith('results/') and (ROOT/p).is_file()}
assert os.environ.get('SUSS_ASSERT_TARGET'), 'Choose an isolated owned target'
for name in ['cases.json','command.json','oracle.clj','upstream-assert.clj','assert-adapted.sus','assert.patch']:
 p=BASE/name;inputs[str(p.relative_to(ROOT))]=hashlib.sha256(p.read_bytes()).hexdigest()
env=os.environ|{'CARGO_TARGET_DIR':os.environ['SUSS_ASSERT_TARGET'],'CARGO_BUILD_JOBS':'1','RUSTUP_TOOLCHAIN':'stable','XDG_RUNTIME_DIR':'/run/user/1000'}
record={'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'command':command,'inputs':inputs,'runner_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'TMPDIR':env.get('TMPDIR'),'complete':False}
def save():(OUT/'command.json').write_text(json.dumps(record,indent=2)+'\n')
save();started=time.time()
with (OUT/'run.log').open('w') as log:
 result=subprocess.run(['systemd-run','--user','--scope','-p','MemoryMax=3G','--',*command],cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT)
record.update(complete=True,exit_code=result.returncode,seconds=time.time()-started,inputs_unchanged=all(hashlib.sha256((ROOT/p).read_bytes()).hexdigest()==h for p,h in inputs.items()));save()
print({k:v for k,v in record.items() if k!='inputs'},flush=True);raise SystemExit(result.returncode)
