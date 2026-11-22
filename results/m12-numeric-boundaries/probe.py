from pathlib import Path
import hashlib,json,os,subprocess,time
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
expressions=[
 '(let [x (long 1)] [(= x 1N) (= (hash x) (hash 1N)) (< x 3/2) (== x 4/2) (+ x 9223372036854775808N)])',
 '(let [x (identity 1)] [(= x 1N) (= (hash x) (hash 1N)) (< x 3/2) (== x 4/2) (+ x 9223372036854775808N)])',
 '(let [x (long 9223372036854775807)] (inc\' x))',
 '(let [x (long 1)] (unchecked-add x 9223372036854775808N))',
 '((fn [^double x] x) 1N)',
 '((fn [^double x] x) 1/7)',
 '((fn [^long x] x) 7/3)',
 '((fn [^long x] x) 9223372036854775808N)',
 '((fn ^double [x] x) 1/7)',
 '((fn ^long [x] x) 7/3)',
 '((fn ^long [x] x) 9223372036854775808N)',
 '((fn [^long x] (/ x 3)) 2)',
 '(let [x 9223372036854775808N f (fn [] (+ x 1))] (f))',
 '(let [x 1/3 f (fn [] (+ x 1))] (f))',
 '(let [x 1/3] (with-redefs [= (fn [& xs] false)] (case x 1/3 :yes :no)))',
 '(let [x 1N] (with-redefs [= (fn [& xs] false)] (case x 1N :yes :no)))',
]
forms=[f'(pr-str (try {form} (catch :default e :error)))' for form in expressions]
source='(assert (= "1.12.6" (clojure-version)))\n'+'\n'.join('(println (pr-str '+form.replace('(catch :default e','(catch Throwable e')+'))' for form in forms)+'\n'
(OUT/'oracle.clj').write_text(source);prior=json.loads((ROOT/'results/m12-ratio-values/oracle-command.json').read_text());command=prior['command'][:-1]+[str(OUT/'oracle.clj')]
start=time.time();r=subprocess.run(command,cwd=ROOT,capture_output=True,text=True);(OUT/'oracle.log').write_text(r.stdout+r.stderr)
(OUT/'oracle-command.json').write_text(json.dumps({'command':command,'exit_code':r.returncode,'seconds':time.time()-start,'source_sha256':hashlib.sha256(source.encode()).hexdigest(),'jars':prior['jars']},indent=2)+'\n');assert r.returncode==0,r.stderr
expected=[json.loads(line) for line in r.stdout.splitlines()];assert len(expected)==len(forms)
(OUT/'cases.json').write_text(json.dumps(list(zip(forms,expected)),indent=2)+'\n')
program='(ns numeric.boundaries)\n(def check (fn [index] (case index '+ ' '.join(str(i)+' '+form for i,form in enumerate(forms))+' \"invalid\")))\n'
(OUT/'probe.sus').write_text(program)
(OUT/'probe.wit').write_text('package test:numeric-boundaries; world w { export check: func(index: u32) -> string; }\n')
binary=ROOT.parents[2]/'target/probe/m12-ratio-target/debug/suss'
scratch=ROOT.parents[2]/'target/probe/m12-numeric-boundaries-84';scratch.mkdir(exist_ok=True);artifact=scratch/'probe.wasm'
command=[str(binary),'compile',str(OUT/'probe.sus'),'-w',str(OUT/'probe.wit'),'-o',str(artifact),'--export','check=numeric.boundaries/check']
start=time.time();r=subprocess.run(command,cwd=ROOT,capture_output=True,text=True)
record={'mode':'actual AOT component','head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'executable_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'source_sha256':hashlib.sha256(program.encode()).hexdigest(),'build':{'command':command,'exit_code':r.returncode,'stdout':r.stdout,'stderr':r.stderr,'seconds':time.time()-start},'calls':[],'expected':expected,'complete':False}
(OUT/'execution.json').write_text(json.dumps(record,indent=2)+'\n')
assert r.returncode==0,r.stderr
record['artifact_sha256']=hashlib.sha256(artifact.read_bytes()).hexdigest()
for index in range(len(forms)):
 command=[str(binary),'run',str(artifact),'--invoke','check','--',str(index)];start=time.time();r=subprocess.run(command,cwd=ROOT,capture_output=True,text=True)
 record['calls'].append({'command':command,'exit_code':r.returncode,'stdout':r.stdout,'stderr':r.stderr,'seconds':time.time()-start})
actual=[row['stdout'].rstrip('\n') for row in record['calls']]
record.update(actual=actual,complete=True,all_match=all(row['exit_code']==0 for row in record['calls']) and actual==expected)
(OUT/'execution.json').write_text(json.dumps(record,indent=2)+'\n')
print('16 fresh primary cases; AOT all_match',record['all_match'])
for i,(a,b) in enumerate(zip(actual,expected)):
 if a!=b:print(i,expressions[i],'actual',a,'primary',b)
