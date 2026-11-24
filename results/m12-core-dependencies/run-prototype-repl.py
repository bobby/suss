"""Execute every control as a separate ordinary compiled REPL request."""
from pathlib import Path
import hashlib,json,os,subprocess,time
ROOT=Path(__file__).resolve().parents[2];BASE=Path(__file__).resolve().parent;OUT=BASE/'prototype-repl';OUT.mkdir(exist_ok=True);assert not (OUT/'command.json').exists()
cases=json.loads((BASE/'probe-cases.json').read_text())
source=OUT/'stdin.sus';source.write_text((BASE/'min-key-adapted.sus').read_text()+(BASE/'some-fn-adapted.sus').read_text()+''.join('(pr-str '+row['expression'].replace('(catch Throwable ','(catch :default ')+')\n' for row in cases))
exe=ROOT.parent/'m12-suite-platform-target/debug/suss';exe_hash=hashlib.sha256(exe.read_bytes()).hexdigest();command=[str(exe),'--no-cache','repl','--compile-fuel','10000000']
record={'command':command,'source_head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'executable_sha256':exe_hash,'input_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'primary_sha256':hashlib.sha256((BASE/'cases.json').read_bytes()).hexdigest(),'probe_inputs_sha256':hashlib.sha256((BASE/'probe-cases.json').read_bytes()).hexdigest(),'complete':False,'qualification':'Each control is an individual REPL request with explicit ordinary 10M compilation allowance; local definitions only, not production core/Macro/AOT acceptance. Six pure fixture constructors have separate actual JVM equivalence evidence.'}
def save():(OUT/'command.json').write_text(json.dumps(record,indent=2)+'\n')
save();start=time.time()
with source.open('r') as stdin,(OUT/'stdout.log').open('w') as stdout,(OUT/'stderr.log').open('w') as stderr:
 r=subprocess.run(['systemd-run','--user','--scope','-p','MemoryMax=3G','--',*command],cwd=ROOT,env=os.environ|{'XDG_RUNTIME_DIR':'/run/user/1000'},stdin=stdin,stdout=stdout,stderr=stderr)
lines=(OUT/'stdout.log').read_text().splitlines();prefix=lines[:2];values=[];decode_errors=[]
for i,line in enumerate(lines[2:]):
 try:values.append(json.loads(line))
 except (ValueError,TypeError):values.append(None);decode_errors.append({'index':i,'line':line})
record.update(complete=True,exit_code=r.returncode,seconds=time.time()-start,observed_rows=len(values),expected_rows=len(cases),prefix=prefix,prefix_matches=prefix==["#'user/min-key","#'user/some-fn"],executable_unchanged=hashlib.sha256(exe.read_bytes()).hexdigest()==exe_hash,decode_errors=decode_errors)
record['mismatches']=[{'index':i,'name':row['name'],'expected':row['expected'],'actual':values[i] if i<len(values) else None} for i,row in enumerate(cases) if i>=len(values) or values[i]!=row['expected']];record['extra_rows']=values[len(cases):];save();print({k:v for k,v in record.items() if k not in ['mismatches','command','decode_errors']},flush=True);print('mismatches',len(record['mismatches']),flush=True)
raise SystemExit(r.returncode or int(not record['prefix_matches'] or bool(record['mismatches'] or record['extra_rows'] or decode_errors)))
