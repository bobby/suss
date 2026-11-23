"""Run complete primary both-phase arithmetic observations with frozen inputs."""
from pathlib import Path
import hashlib,json,os,subprocess,time,sys
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
label=sys.argv[1]
assert not (OUT/(label+'.json')).exists()
command=['cargo','test','--locked','-p','suss-cli','--test','clojure_decimal_arithmetic','--','--test-threads=2']
if len(sys.argv)>2:
 filters=sys.argv[2].split(',')
 command[command.index('--')+1:command.index('--')+1]=filters
paths=subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z'],cwd=ROOT).decode().split('\0')
inputs={p:hashlib.sha256((ROOT/p).read_bytes()).hexdigest() for p in paths if p and not p.startswith('results/') and (ROOT/p).is_file()}
inputs['results/m12-decimal-arithmetic/cases.json']=hashlib.sha256((ROOT/'results/m12-decimal-arithmetic/cases.json').read_bytes()).hexdigest()
for p in OUT.glob('*cases.json'):inputs[str(p.relative_to(ROOT))]=hashlib.sha256(p.read_bytes()).hexdigest()
env=os.environ|{'CARGO_TARGET_DIR':str(ROOT.parent/'m12-decimal-target'),'CARGO_BUILD_JOBS':'2','RUSTUP_TOOLCHAIN':'stable','XDG_RUNTIME_DIR':'/run/user/1000'}
record={'base':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'inputs':inputs,'command':command,'complete':False}
def save():(OUT/(label+'.json')).write_text(json.dumps(record,indent=2)+'\n')
save();started=time.time()
with (OUT/(label+'.log')).open('w') as log:r=subprocess.run(['systemd-run','--user','--scope','-p','MemoryMax=4G','--',*command],cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT)
record.update(complete=True,exit_code=r.returncode,seconds=time.time()-started,inputs_unchanged=all(hashlib.sha256((ROOT/p).read_bytes()).hexdigest()==h for p,h in inputs.items()));save()
print({k:v for k,v in record.items() if k!='inputs'},flush=True)
