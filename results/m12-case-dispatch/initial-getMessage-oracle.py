from pathlib import Path
import hashlib,json,subprocess,time
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
sources=[]
selectors=['1','1N','(biginteger 1)','1.0','1M','1.00M']
for value in selectors:
 for constant in ['1','1N','1.0','1M','1.00M']:
  sources.append(f'(case {value} {constant} :hit :miss)')
for value in ['0','0.0','-0.0']:
 for constant in ['0','0.0','-0.0']:
  sources.append(f'(case {value} {constant} :hit :miss)')
sources += [
 '(case \'name name :hit :miss)', '(case \'app/name app/name :hit :miss)',
 '(case \'quote \'foo :hit :miss)', '(case \'foo \'foo :hit :miss)',
 '(case \'(a b) ((a b)) :hit :miss)', '(case \'(a b) [a b] :hit :miss)',
 '(case [\'a \'b] ((a b)) :hit :miss)', '(case [1N] [1] :hit :miss)',
 '(case {:a 1N :b 2} {:b 2 :a 1} :hit :miss)', '(case #{1N 2} #{2 1} :hit :miss)',
 '(case [1.00M] [1.0M] :hit :miss)', '(case {:a 1.00M} {:a 1.0M} :hit :miss)',
 '(case #{1.00M} #{1.0M} :hit :miss)', '(case ^:m [1M] [1M] :hit :miss)',
 '(case [1M] ^:m [1M] :hit :miss)', '(case [{:a #{1M}}] [{:a #{1M}}] :hit :miss)',
 '(case ##NaN ##NaN :hit :miss)', '(case (/ 0.0 0.0) ##NaN :hit :miss)',
 '(case 9223372036854775807N 9223372036854775807 :hit :miss)',
 '(case (biginteger 1000000000000N) 1000000000000 :hit :miss)',
 '(case (biginteger -1) -1 :hit :miss)', '(case (biginteger -1) -1 :hit nil :nil :miss)',
 '(case 1/2 1/2 :hit :miss)', '(case 0.5M 1/2 :hit :miss)',
 '(case :a (:b :a :c) :hit :miss)', '(case :missing (:b :a :c) :hit :miss)',
 '(case true false :no true :yes :default)', '(case nil false :no nil :yes :default)',
 '(case [1] [1] :hit)', '(case :a :a :hit)', '(case :missing :default)',
 '(with-redefs [= (fn [a b] (throw (ex-info "eq-called" {})))] (case true false :no true :yes :default))',
 '(with-redefs [keyword? (fn [x] false)] (case :hit :hit :yes :default))',
 '(with-redefs [= (fn [a b] (throw (ex-info "eq-called" {})))] (case 0 nil :nil 0 :zero :default))',
 '(with-redefs [= (fn [a b] (throw (ex-info "eq-called" {})))] (case [1] [1] :hit :miss))',
 '(with-redefs [= (fn [a b] (throw (ex-info "eq-called" {})))] (case {:a [1M]} {:a [1M]} :hit :miss))',
 '(with-redefs [hash (fn [a] (throw (ex-info "hash-called" {})))] (case [1M] [1M] :hit :miss))',
 '(with-redefs [str (fn [& xs] "replacement")] (try (case :x) (catch Throwable e (.getMessage e))))',
 '(do (def case-effects []) (let [r (case (do (def case-effects (conj case-effects :selector)) [1N]) [1] (do (def case-effects (conj case-effects :chosen)) :hit) (do (def case-effects (conj case-effects :default)) :miss))] [r case-effects]))',
 '(let [f (fn [x] (case x [1M] :hit :miss))] (f [1M]))',
 '(loop [i 0] (case i 0 (recur 1) 1 :hit :miss))',
 '(let [v :captured f (fn [x] (case x [1M] (fn [] v) (fn [] :miss)))] ((f [1M])))',
]
for value in ['nil','false','true','1','1.50M','1/3','[1 2]','{:a 1}','#{1 2}','"x"',"'name",'##NaN']:
 sources.append(f'(case {value})')
# These are language execution controls, not duplicate/expansion diagnostics.
source='(assert (= "1.12.6" (clojure-version)))\n'+''.join('(println (pr-str (pr-str (try '+s+' (catch Throwable e [:error (.getMessage e)])))))\n' for s in sources)
(OUT/'oracle.clj').write_text(source)
prior=json.loads((ROOT/'results/m12-wide-wit/oracle-command.json').read_text());command=prior['command'][:-1]+[str(OUT/'oracle.clj')]
t=time.time();r=subprocess.run(command,cwd=ROOT,text=True,capture_output=True)
(OUT/'oracle.log').write_text(r.stdout+r.stderr);(OUT/'oracle-command.json').write_text(json.dumps({'command':command,'exit_code':r.returncode,'seconds':time.time()-t,'source_sha256':hashlib.sha256(source.encode()).hexdigest(),'jars':{p:hashlib.sha256(Path(p).read_bytes()).hexdigest() for p in prior['jars']}},indent=2)+'\n')
assert r.returncode==0,r.stderr
values=[json.loads(x) for x in r.stdout.splitlines()];assert len(values)==len(sources)
(OUT/'cases.json').write_text(json.dumps(list(zip(sources,values)),indent=2)+'\n');print(len(values),'fresh case controls')
