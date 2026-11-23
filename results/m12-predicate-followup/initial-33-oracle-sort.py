"""Pinned JVM controls for the next unresolved dependency, public sort."""
from pathlib import Path
import hashlib,json,subprocess,time
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
expressions=['(sort nil)','(sort [])','(sort [3 1 2])','(sort > [3 1 2])','(sort < [3 1 2 1])','(sort [3M 1.0M 2M 1.00M])','(sort [3 1N 2/3 0.5 1.0M])','(sort [nil 2 nil 1])','(sort ["z" "a" "b"])','(sort [false true false])','(sort [:b :a])','(sort [\'b \'a])','(sort (with-meta [2 1] {:a 1}))','(meta (sort (with-meta [2 1] {:a 1})))','(meta (sort (with-meta [] {:a 1})))','(sort (fn [a b] (compare (first a) (first b))) [[1 :a] [0 :b] [1 :c] [0 :d]])','(sort (fn [a b] (< (first a) (first b))) [[1 :a] [0 :b] [1 :c] [0 :d]])']
for value in ['0','0.5','-0.5','0.5M','-0.5M','1/3','-1/3','0N','4294967296N','nil','"x"',':x','[]']:
 expressions.append('(sort (fn [a b] '+value+') [3 1 2])')
expressions += ['(do (def sort-effects []) (let [r (sort (do (def sort-effects (conj sort-effects :comparator)) <) (do (def sort-effects (conj sort-effects :collection)) [3 1 2]))] [(vec r) sort-effects]))','(let [f sort] (f [3 1 2]))','(apply sort [< [3 1 2]])']
source='(assert (= "1.12.6" (clojure-version)))\n'+''.join('(println (pr-str (pr-str (try '+e+' (catch Throwable e [:error (.getMessage e)])))))\n' for e in expressions)
(OUT/'sort-oracle.clj').write_text(source)
prior=json.loads((ROOT/'results/m12-wide-wit/oracle-command.json').read_text());command=prior['command'][:-1]+[str(OUT/'sort-oracle.clj')]
t=time.time();r=subprocess.run(command,cwd=ROOT,text=True,capture_output=True)
(OUT/'sort-oracle.log').write_text(r.stdout+r.stderr)
(OUT/'sort-oracle-command.json').write_text(json.dumps({'command':command,'exit_code':r.returncode,'seconds':time.time()-t,'source_sha256':hashlib.sha256(source.encode()).hexdigest(),'jars':{p:hashlib.sha256(Path(p).read_bytes()).hexdigest() for p in prior['jars']}},indent=2)+'\n')
assert r.returncode==0,r.stderr
values=[json.loads(x) for x in r.stdout.splitlines()];assert len(values)==len(expressions)
(OUT/'sort-cases.json').write_text(json.dumps(list(zip(expressions,values)),indent=2)+'\n')
for i,(source,value) in enumerate(zip(expressions,values)):print(i,source,'=>',value)
