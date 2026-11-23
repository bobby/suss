"""Fresh signed-half boundaries, collection keys and literal transport controls."""
from pathlib import Path
import json,hashlib,subprocess,time
BASE=Path(__file__).resolve().parent;old=json.loads((BASE/'reader-oracle-command.json').read_text());cases=[]
def u(s):return '#uuid '+json.dumps(s)
def add(s,j=None):cases.append({'expression':s,'jvm_expression':j or s})
values=['7fffffff-ffff-fffe-0000-000000000000','7fffffff-ffff-ffff-0000-000000000000','80000000-0000-0000-0000-000000000000','80000000-0000-0001-0000-000000000000','00000000-0000-0000-7fff-fffffffffffe','00000000-0000-0000-7fff-ffffffffffff','00000000-0000-0000-8000-000000000000','00000000-0000-0000-8000-000000000001','00200000-0000-0000-0000-000000000000','00200000-0000-0001-0000-000000000000']
for i in range(0,len(values),2):
 a,b=map(u,values[i:i+2]);add(f'[(= {a} {b}) (compare {a} {b}) (compare {b} {a}) (count (sorted-set {a} {b}))]')
a=u('1-2-3-4-5');b=u('00000001-0002-0003-0004-000000000005')
add(f'(let [x (read-string {json.dumps(a)})] [(identical? x x) (identical? x (read-string {json.dumps(a)})) (= x {b}) (hash x)])')
add(f'(let [{{x {a} :as m}} {{{b} 42}}] [x (count m)])')
add(f'(uuid? `{a})')
add(f'[(get {{{a} 9}} {b}) (count (set [{a} {b}])) (case {a} {b} :hit :miss)]')
for text in ['#{'+a+' '+b+'}','{'+a+' 1 '+b+' 2}']:
 add(f'(try (read-string {json.dumps(text)}) (catch Throwable e [:error (ex-message e)]))')
add('(mapv uuid? [nil true 0 0.0 0N 0M 1/3 :uuid \'uuid "uuid" [] {} #{}])')
source=BASE/'uuid-boundaries-oracle.clj';source.write_text('(assert (= "1.12.6" (clojure-version)))\n'+''.join('(println (pr-str (pr-str '+r['jvm_expression']+')))\n' for r in cases))
cmd=old['command'][:-1]+[str(source)];start=time.time();r=subprocess.run(cmd,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True);(BASE/'uuid-boundaries-oracle.log').write_text(r.stdout)
d={'command':cmd,'exit_code':r.returncode,'seconds':time.time()-start,'jars':old['jars'],'source_sha256':hashlib.sha256(source.read_bytes()).hexdigest()}
if r.returncode==0:
 rows=[json.loads(line) for line in r.stdout.splitlines()];assert len(rows)==len(cases)
 for row,expected in zip(cases,rows):row['expected']=expected
 (BASE/'uuid-boundary-cases.json').write_text(json.dumps(cases,indent=2)+'\n');d['cases']=len(cases);d['output_sha256']=hashlib.sha256((BASE/'uuid-boundary-cases.json').read_bytes()).hexdigest()
(BASE/'uuid-boundaries-oracle-command.json').write_text(json.dumps(d,indent=2)+'\n');print(r.stdout[-1200:]);raise SystemExit(r.returncode)
