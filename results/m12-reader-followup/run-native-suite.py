"""Reexecute the qualified native suite without contending for Cargo's lock."""
from pathlib import Path
import hashlib,json,os,subprocess,time
ROOT=Path(__file__).resolve().parents[2];BASE=Path(__file__).resolve().parent
OUT=BASE/'native-suite-after-reviewed-diagnostics';OUT.mkdir(exist_ok=True)
assert not (OUT/'command.json').exists()
previous=json.loads((BASE/'full-suite-after-inline-source-macros/suite-commands.json').read_text())
command=next(row['command'] for row in previous['commands'] if row['label']=='suite-run')
executable=Path(command[0]);digest=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
assert digest(executable)==previous['executable_sha256']
paths=subprocess.check_output(['git','ls-files','-z'],cwd=ROOT).decode().split('\0')
inputs={p:digest(ROOT/p) for p in paths if p and (ROOT/p).is_file()}
core='clojurescript/src/main/cljs/cljs/core.cljs';inputs[core]=digest(ROOT/core)
record={'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'command':command,'executable_sha256':digest(executable),'inputs':inputs,'complete':False}
def save():(OUT/'command.json').write_text(json.dumps(record,indent=2)+'\n')
save();env=os.environ|{'XDG_RUNTIME_DIR':'/run/user/1000','SUSS_CLOJURE_TEST_SUITE_CAPTURE':str(OUT/'suite-capture')}
env.pop('SUSS_CLOJURE_TEST_SUITE_WRITE',None)
started=time.time()
with (OUT/'run.log').open('w') as log:r=subprocess.run(['systemd-run','--user','--scope','-p','MemoryMax=3G','--',*command],cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT)
record.update(complete=True,exit_code=r.returncode,seconds=time.time()-started,inputs_unchanged=all((ROOT/p).is_file() and digest(ROOT/p)==h for p,h in inputs.items()),executable_unchanged=digest(executable)==record['executable_sha256']);save()
print({k:v for k,v in record.items() if k!='inputs'},flush=True)
raise SystemExit(r.returncode or int(not record['inputs_unchanged'] or not record['executable_unchanged']))
