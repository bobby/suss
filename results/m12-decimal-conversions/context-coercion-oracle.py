from pathlib import Path
import hashlib,json,subprocess,time
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
expressions=['(pr-str (binding [*math-context* :invalid] (mapv bigdec [1 1N (biginteger 1)])))', '(pr-str (binding [*math-context* :invalid] [(bigdec "1.234") (bigdec 0.1) (bigdec 1.234M)]))', '(pr-str (binding [*math-context* :invalid] [(= 1M 1.00M) (== 1M 1) (compare 1M 1) (< 1M 2) (zero? 0.00M) (pos? 1E-1000M) (neg? -1E-1000M)]))', '(pr-str (binding [*math-context* :invalid] [(hash 1.00M) (case 1.0M 1.0M :same :different) (rationalize 1.25M) (unchecked-long 1.99M)]))', '(try (binding [*math-context* :invalid] (bigdec 1/2)) :accepted (catch Throwable e :rejected))', '(try (binding [*math-context* :invalid] (== 0.5M 1/2)) :accepted (catch Throwable e :rejected))', '(try (binding [*math-context* :invalid] (+ 1M 1M)) :accepted (catch Throwable e :rejected))', '(pr-str (binding [*math-context* :invalid] [(+ 1.234M) (* 1.234M) (decimal? 1M)]))']
source='(assert (= "1.12.6" (clojure-version)))\n'+''.join('(println (pr-str (pr-str (try '+e+' (catch Throwable e [:error (.getMessage e)])))))\n' for e in expressions)
(OUT/'context-coercion-oracle.clj').write_text(source)
prior=json.loads((ROOT/'results/m12-wide-wit/oracle-command.json').read_text());command=prior['command'][:-1]+[str(OUT/'context-coercion-oracle.clj')]
t=time.time();r=subprocess.run(command,cwd=ROOT,text=True,capture_output=True)
(OUT/'context-coercion-oracle.log').write_text(r.stdout+r.stderr)
(OUT/'context-coercion-oracle-command.json').write_text(json.dumps({'command':command,'exit_code':r.returncode,'seconds':time.time()-t,'source_sha256':hashlib.sha256(source.encode()).hexdigest(),'jars':{p:hashlib.sha256(Path(p).read_bytes()).hexdigest() for p in prior['jars']}},indent=2)+'\n')
assert r.returncode==0,r.stderr
values=[json.loads(x) for x in r.stdout.splitlines()];assert len(values)==len(expressions)
(OUT/'context-coercion-cases.json').write_text(json.dumps(list(zip(expressions,values)),indent=2)+'\n')
for i,x in enumerate(values):print(i,x)
