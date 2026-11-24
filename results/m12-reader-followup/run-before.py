"""Execute copied, hash-pinned pre-repair CLI; source forms are never rewritten."""
from pathlib import Path
import hashlib,json,os,subprocess,time
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent/'before-native-readers';OUT.mkdir(exist_ok=True);assert not (OUT/'commands.json').exists()
exe=ROOT/'target/probe/suss-before-reader';exe_hash=hashlib.sha256(exe.read_bytes()).hexdigest();env=os.environ|{'XDG_RUNTIME_DIR':'/run/user/1000'}
cases=json.loads((OUT.parent/'reader-cases.json').read_text());selected=cases[:9]+[x for x in cases if x['name'] in ['regex-str-0','regex-self-0','regex-distinct-0']]
record={'source_head':'2254f70cb62c04741f84dce6ce7724866c3f77a8','executable_sha256':exe_hash,'commands':[],'complete':False}
def save():(OUT/'commands.json').write_text(json.dumps(record,indent=2)+'\n')
def run(label,args,source=None):
 command=[str(exe),'--build-dir',str(ROOT/'target/probe/reader-before-build'),*args];started=time.time();r=subprocess.run(['systemd-run','--user','--scope','-p','MemoryMax=2G','--',*command],cwd=ROOT,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True);(OUT/(label+'.log')).write_text(r.stdout);record['commands'].append({'label':label,'command':command,'expression':source,'exit_code':r.returncode,'seconds':time.time()-started,'stdout_sha256':hashlib.sha256(r.stdout.encode()).hexdigest(),'executable_unchanged':hashlib.sha256(exe.read_bytes()).hexdigest()==exe_hash});save();print(label,r.returncode,flush=True)
save()
for case in selected:run(case['name'],['-e',case['expression']],case['expression'])
source=OUT/'uuid.sus';source.write_text('(def check (fn [unused] (if (= #uuid "1-2-3-4-5" #uuid "00000001-0002-0003-0004-000000000005") 1 0)))\n');wit=OUT/'probe.wit';wit.write_text('package test:uuid; world w { export check: func(unused: u32) -> u32; }\n')
run('uuid-aot',['compile',str(source),'-w',str(wit),'--export','check=check','-o',str(OUT/'uuid.wasm')])
record['complete']=True;save()
