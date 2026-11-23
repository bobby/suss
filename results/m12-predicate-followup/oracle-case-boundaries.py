from pathlib import Path
import hashlib,json,subprocess,time
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
sources=[
 '(case 1.00M 1.0M :hit :miss)',
 '(case [1.00M] [1.0M] :hit :miss)',
 '(case {:x 1.00M} {:x 1.0M} :hit :miss)',
 '(case #{1.00M} #{1.0M} :hit :miss)',
 '(case ##NaN ##NaN :hit :miss)',
 '(case (/ 0.0 0.0) ##NaN :hit :miss)',
 '(case 0.0 -0.0 :hit :miss)',
 '(case 1N 1 :hit :miss)',
 '(case 1.0M 1.0M :a 1.00M :b :default)',
 '(case [1.0M] [1.0M] :a [1.00M] :b :default)',
 '(case [1] [1] :a ((1N)) :b :default)',
 '(case :x () unknown-symbol :default)',
 '(case :x)',
 '(case 1.50M)',
 '(case [1 2])',
 '(case \'name name :hit :miss)',
 '(case \'quote \'foo :hit :miss)',
 '(with-redefs [= (fn [a b] (throw (ex-info "eq-called" {})))] (case true false :no true :yes :default))',
 '(with-redefs [keyword? (fn [x] false)] (case :hit :hit :yes :default))',
 '(with-redefs [= (fn [a b] (throw (ex-info "eq-called" {})))] (case 0 nil :nil 0 :zero :default))',
 '(with-redefs [= (fn [a b] (throw (ex-info "eq-called" {})))] (case [1] [1] :hit :miss))',
 '(with-redefs [str (fn [& xs] "replacement")] (try (case :x) (catch Throwable e (.getMessage e))))',
]
source='(assert (= "1.12.6" (clojure-version)))\n'
for s in sources:
 source+='(println (pr-str (pr-str (try (eval (read-string '+json.dumps(s)+')) (catch Throwable e (let [cause (loop [e e] (if (.getCause e) (recur (.getCause e)) e))] [:error (.getName (class cause)) (.getMessage cause)]))))))\n'
(OUT/'case-boundaries-oracle.clj').write_text(source)
prior=json.loads((ROOT/'results/m12-wide-wit/oracle-command.json').read_text());command=prior['command'][:-1]+[str(OUT/'case-boundaries-oracle.clj')]
t=time.time();r=subprocess.run(command,cwd=ROOT,text=True,capture_output=True)
(OUT/'case-boundaries-oracle.log').write_text(r.stdout+r.stderr);(OUT/'case-boundaries-oracle-command.json').write_text(json.dumps({'command':command,'exit_code':r.returncode,'seconds':time.time()-t,'source_sha256':hashlib.sha256(source.encode()).hexdigest(),'jars':{p:hashlib.sha256(Path(p).read_bytes()).hexdigest() for p in prior['jars']}},indent=2)+'\n')
assert r.returncode==0,r.stderr
values=[json.loads(x) for x in r.stdout.splitlines()];assert len(values)==len(sources)
(OUT/'case-boundaries-primary.json').write_text(json.dumps(list(zip(sources,values)),indent=2)+'\n')
for s,v in zip(sources,values):print(s,'=>',v)
