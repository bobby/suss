"""Run the required unfiltered workspace command against frozen source."""
from pathlib import Path
import hashlib,json,os,subprocess,time
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
command=['cargo','test','--workspace','--locked','--','--test-threads=2']
ENV=os.environ|{'CARGO_BUILD_JOBS':'2','RUSTUP_TOOLCHAIN':'stable','XDG_RUNTIME_DIR':'/run/user/1000','CARGO_TARGET_DIR':str(ROOT.parent/'m12-helper-target'),'SUSS_CLOJURE_TEST_SUITE_CAPTURE':str(OUT/'suite-capture')}
ENV.pop('SUSS_CLOJURE_TEST_SUITE_WRITE',None)
paths=subprocess.check_output(['git','ls-files','-z'],cwd=ROOT).decode().split('\0')
inputs={p:hashlib.sha256((ROOT/p).read_bytes()).hexdigest() for p in paths if p and not p.startswith('results/') and (ROOT/p).is_file()}
record={'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'submodule_head':subprocess.check_output(['git','-C','clojurescript','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'inputs':inputs,'command':command,'environment':{'CARGO_BUILD_JOBS':'2','RUSTUP_TOOLCHAIN':'stable','scope':'MemoryMax=4G','CARGO_TARGET_DIR':ENV['CARGO_TARGET_DIR']},'complete':False}
def save():(OUT/'command.json').write_text(json.dumps(record,indent=2)+'\n')
save();started=time.time()
with (OUT/'run.log').open('w') as log:r=subprocess.run(['systemd-run','--user','--scope','-p','MemoryMax=4G','--',*command],cwd=ROOT,env=ENV,stdout=log,stderr=subprocess.STDOUT)
record.update(exit_code=r.returncode,seconds=time.time()-started,complete=True,inputs_unchanged=all(hashlib.sha256((ROOT/p).read_bytes()).hexdigest()==digest for p,digest in inputs.items()));save();print({k:v for k,v in record.items() if k not in ['inputs']},flush=True)
