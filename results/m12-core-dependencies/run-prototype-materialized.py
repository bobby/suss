"""Exercise retained upstream forms with the current compiler, without porting core."""
from pathlib import Path
import hashlib,json,os,subprocess,time
ROOT=Path(__file__).resolve().parents[2];BASE=Path(__file__).resolve().parent
OUT=BASE/'prototype-materialized-fixtures';OUT.mkdir(exist_ok=True);assert not (OUT/'command.json').exists()
cases=json.loads((BASE/'probe-cases.json').read_text())
notice=';; Copyright (c) Rich Hickey. All rights reserved.\n;; EPL-1.0; see runtime/core-import/epl-v10.html.\n;; Retained Clojure 1.12.6 functions in a probe namespace, not a core port.\n'
source=OUT/'probe.sus';source.write_text(notice+'(do\n'+(BASE/'min-key-adapted.sus').read_text()+(BASE/'some-fn-adapted.sus').read_text()+'(str\n'+''.join('(pr-str '+row['expression'].replace('(catch Throwable ','(catch :default ')+') \"\\n\"\n' for row in cases)+'))\n')
exe=ROOT.parent/'m12-suite-platform-target/debug/suss';exe_hash=hashlib.sha256(exe.read_bytes()).hexdigest()
command=[str(exe),'--no-cache','-e',source.read_text()]
record={'command':command,'source_head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'executable_sha256':exe_hash,'probe_source_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'primary_sha256':hashlib.sha256((BASE/'probe-cases.json').read_bytes()).hexdigest(),'qualification':'Local definitions of retained Clojure functions only; not production core or a Macro/AOT acceptance claim.','complete':False}
def save():(OUT/'command.json').write_text(json.dumps(record,indent=2)+'\n')
save();start=time.time()
with (OUT/'stdout.log').open('w') as stdout,(OUT/'stderr.log').open('w') as stderr:
 r=subprocess.run(['systemd-run','--user','--scope','-p','MemoryMax=3G','--',*command],cwd=ROOT,env=os.environ|{'XDG_RUNTIME_DIR':'/run/user/1000'},stdout=stdout,stderr=stderr)
output=(OUT/'stdout.log').read_text();lines=json.loads(output).splitlines() if r.returncode==0 else []
record.update(complete=True,exit_code=r.returncode,seconds=time.time()-start,observed_rows=len(lines),expected_rows=len(cases),executable_unchanged=hashlib.sha256(exe.read_bytes()).hexdigest()==exe_hash)
record['mismatches']=[{'index':i,'name':row['name'],'expected':row['expected'],'actual':lines[i] if i<len(lines) else None} for i,row in enumerate(cases) if i>=len(lines) or lines[i]!=row['expected']]
record['extra_rows']=lines[len(cases):];save();print({k:v for k,v in record.items() if k not in ['mismatches','command']},flush=True);print('mismatches',len(record['mismatches']),flush=True)
raise SystemExit(r.returncode or int(bool(record['mismatches'] or record['extra_rows'])))
