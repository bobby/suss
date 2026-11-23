from pathlib import Path
import hashlib,json,subprocess,time
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
expressions=['(with-precision 2 (pr-str (let [f -] (f 1.0 1.234M))))', '(with-precision 2 (pr-str ((fn [x] (- x 1.234M)) 1.0)))', '(with-precision 2 (pr-str (let [^double x 1.0] (- x 1.234M))))', '(with-precision 2 (pr-str (let [x 1.0] (- x 1.234M))))', '(with-precision 2 (pr-str (- 1/3 1.234M)))', '(pr-str (- 1M -9223372036854775808))', '(pr-str (let [f -] (f 1M -9223372036854775808)))', '(pr-str (let [f -] (f -9223372036854775808 -9223372036854775808)))', '(pr-str (- -9223372036854775808 -9223372036854775808))', '(with-precision 2 (pr-str (let [f unchecked-subtract] (f 1.0 1.234M))))', "(with-precision 2 (pr-str [(unchecked-subtract 1.0 1.234M) (-' 1.0 1.234M)]))", '(pr-str (- 1M 1/3))', '(with-precision 2 :rounding CEILING (pr-str (- 1M 1/3)))']
source='(assert (= "1.12.6" (clojure-version)))\n'+''.join('(println (pr-str (pr-str (try '+e+' (catch Throwable e [:error (.getMessage e)])))))\n' for e in expressions)
(OUT/'subtraction-dispatch-oracle.clj').write_text(source)
prior=json.loads((ROOT/'results/m12-wide-wit/oracle-command.json').read_text());command=prior['command'][:-1]+[str(OUT/'subtraction-dispatch-oracle.clj')]
t=time.time();r=subprocess.run(command,cwd=ROOT,text=True,capture_output=True)
(OUT/'subtraction-dispatch-oracle.log').write_text(r.stdout+r.stderr)
(OUT/'subtraction-dispatch-oracle-command.json').write_text(json.dumps({'command':command,'exit_code':r.returncode,'seconds':time.time()-t,'source_sha256':hashlib.sha256(source.encode()).hexdigest(),'jars':{p:hashlib.sha256(Path(p).read_bytes()).hexdigest() for p in prior['jars']}},indent=2)+'\n')
assert r.returncode==0,r.stderr
values=[json.loads(x) for x in r.stdout.splitlines()];assert len(values)==len(expressions)
(OUT/'subtraction-dispatch-cases.json').write_text(json.dumps(list(zip(expressions,values)),indent=2)+'\n')
for i,x in enumerate(values):print(i,x)
