from pathlib import Path
import hashlib,json,subprocess,time
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
expressions=['(pr-str (with-precision 3 42))', '(pr-str (with-precision 3N 42))', '(pr-str (with-precision 3M 42))', '(pr-str (with-precision 3.0 42))', '(pr-str (with-precision 3/2 42))', '(pr-str (with-precision nil 42))', '(pr-str (with-precision "3" 42))', '(pr-str (with-precision true 42))', '(pr-str (with-precision 2147483647 42))', '(pr-str (with-precision 2147483648 42))', '(pr-str (with-precision 4294967296 42))', '(pr-str (with-precision -4294967296 42))', '(pr-str (with-precision -1 42))']
source='(assert (= "1.12.6" (clojure-version)))\n'+''.join('(println (pr-str (pr-str (try '+e+' (catch Throwable e [:error (.getMessage e)])))))\n' for e in expressions)
(OUT/'precision-types-oracle.clj').write_text(source)
prior=json.loads((ROOT/'results/m12-wide-wit/oracle-command.json').read_text());command=prior['command'][:-1]+[str(OUT/'precision-types-oracle.clj')]
t=time.time();r=subprocess.run(command,cwd=ROOT,text=True,capture_output=True)
(OUT/'precision-types-oracle.log').write_text(r.stdout+r.stderr)
(OUT/'precision-types-oracle-command.json').write_text(json.dumps({'command':command,'exit_code':r.returncode,'seconds':time.time()-t,'source_sha256':hashlib.sha256(source.encode()).hexdigest(),'jars':{p:hashlib.sha256(Path(p).read_bytes()).hexdigest() for p in prior['jars']}},indent=2)+'\n')
assert r.returncode==0,r.stderr
values=[json.loads(x) for x in r.stdout.splitlines()];assert len(values)==len(expressions)
(OUT/'precision-types-cases.json').write_text(json.dumps(list(zip(expressions,values)),indent=2)+'\n')
for i,x in enumerate(values):print(i,x)
