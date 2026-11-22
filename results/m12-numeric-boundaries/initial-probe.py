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
program='(ns numeric.boundaries)\n'+'\n'.join('(println '+form+')' for form in forms)+'\n';(OUT/'probe.sus').write_text(program)
binary=ROOT.parents[2]/'target/probe/m12-ratio-target/debug/suss';command=[str(binary),str(OUT/'probe.sus')];start=time.time();r=subprocess.run(command,cwd=ROOT,capture_output=True,text=True);(OUT/'suss.log').write_text(r.stdout+r.stderr)
actual=r.stdout.splitlines();record={'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'command':command,'executable_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'source_sha256':hashlib.sha256(program.encode()).hexdigest(),'exit_code':r.returncode,'seconds':time.time()-start,'actual':actual,'expected':expected,'complete':True,'all_match':r.returncode==0 and actual==expected};(OUT/'execution.json').write_text(json.dumps(record,indent=2)+'\n')
print('primary exit',0,'Suss exit',r.returncode,'all_match',record['all_match'])
for i,(a,b) in enumerate(zip(actual,expected)):
 if a!=b:print(i,expressions[i],'actual',a,'primary',b)
if r.returncode:print(r.stderr)
