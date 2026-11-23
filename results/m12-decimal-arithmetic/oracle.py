"""Fresh Clojure 1.12.6 expectations for exact Decimal arithmetic and context."""
from pathlib import Path
import hashlib,json,subprocess,time
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
expressions=[
 '(pr-str [(+ 1.50M 2) (- 1.50M 2N) (* 1.5M 2) (/ 1M 8)])',
 '(pr-str [(+ 0.00M 1M) (* 0.00M 1E+20M) (/ 0.00M 2M) (/ 1.00M 2M)])',
 '(pr-str [(/ 1.00M 8M) (/ 100M 2M) (/ 1M 40M) (/ -1M -8M) (/ 1M -8M)])',
 '(pr-str [(+ 123456789012345678901234567890.00M 0.01M) (* 123456789012345678901234567890.00M 1.50M)])',
 '(pr-str [(+ 1M 1/2) (- 1/2 1M) (* 1.50M 1/2) (/ 1.50M 3/2)])',
 '(pr-str [(+ 1M 0.5) (* 0.5 1.50M) (/ 1M 2.0)])',
 '(pr-str [(- 1.50M) (abs -1.50M) (inc 1.50M) (dec 1.50M)])',
 '(/ 1M 3)', '(+ 1M 1/3)', '(/ 1M 0M)', '(/ 0M 0M)',
 '(with-precision 3 (pr-str [(/ 1M 3) (+ 1.2345M 0M) (* 1.2345M 2M) (str *math-context*)]))',
 '(with-precision 3 (pr-str [(- 1.2345M) (abs -1.2345M) (inc 1.2345M) (dec 1.2345M)]))',
 '(with-precision 3 (pr-str [(/ 1.00M 2M) (/ 1M 8M) (/ 100M 2M) (/ 999M 1M)]))',
 '(with-precision 2 (pr-str [(+ 999M 1M) (+ 1.25M -0.25M) (* 1.50M 0M)]))',
 '(with-precision 0 (/ 1M 3))', '(with-precision -1 1M)',
 '(with-precision 1.5 (str *math-context*))',
 '(with-precision 2147483648 (str *math-context*))',
 '(let [x *math-context*] (try (with-precision 2 (/ 1M 0M)) (catch Throwable _)) (identical? x *math-context*))',
 '(with-precision 3 [(str *math-context*) (with-precision 5 (str *math-context*)) (str *math-context*)])',
 '(pr-str [(quot 1.50M 0.4M) (rem 1.50M 0.4M) (mod -1.50M 0.4M)])',
 '(with-precision 2 (pr-str [(quot 123M 2M) (rem 123M 2M)]))',
 '(with-precision 1 (quot 123M 2M))',
 '(with-precision 1 (rem 123M 2M))',
]
for mode in ['UP','DOWN','CEILING','FLOOR','HALF_UP','HALF_DOWN','HALF_EVEN','UNNECESSARY']:
 for expr in [
    '(pr-str [(+ 1.25M 0M) (+ -1.25M 0M) (+ 1.35M 0M) (+ -1.35M 0M)])',
    '(pr-str [(/ 1M 8M) (/ -1M 8M) (/ 1M 3M) (/ -1M 3M)])',
    '(pr-str [(* 9.99M 1M) (* -9.99M 1M) (/ 999M 1M)])',
 ]: expressions.append(f'(with-precision 2 :rounding {mode} {expr})')
for expr in ['(+ 1E+2147483648M 0E+2147483648M)',
             '(* 1E+2147483648M 1E+1M)',
             '(* 0E+2147483648M 1E+1M)',
             '(* 1E-2147483647M 1E-1M)',
             '(* 0E-2147483647M 1E-1M)',
             '(/ 1E+2147483648M 1E-1M)',
             '(/ 1E-2147483647M 1E+1M)']:
 expressions.append('(pr-str '+expr+')')
source='(assert (= "1.12.6" (clojure-version)))\n'
for expr in expressions:
 source+='(println (pr-str (pr-str (try '+expr+' (catch Throwable e [:error (.getMessage e)])))))\n'
(OUT/'oracle.clj').write_text(source)
prior=json.loads((ROOT/'results/m12-wide-wit/oracle-command.json').read_text());command=prior['command'][:-1]+[str(OUT/'oracle.clj')]
start=time.time();r=subprocess.run(command,cwd=ROOT,text=True,capture_output=True)
(OUT/'oracle.log').write_text(r.stdout+r.stderr)
(OUT/'oracle-command.json').write_text(json.dumps({'command':command,'exit_code':r.returncode,'seconds':time.time()-start,
 'source_sha256':hashlib.sha256(source.encode()).hexdigest(),
 'jars':{p:hashlib.sha256(Path(p).read_bytes()).hexdigest() for p in prior['jars']}},indent=2)+'\n')
assert r.returncode==0,r.stderr
values=[json.loads(x) for x in r.stdout.splitlines()];assert len(values)==len(expressions)
(OUT/'cases.json').write_text(json.dumps(list(zip(expressions,values)),indent=2)+'\n')
print(len(values),'fresh Clojure Decimal arithmetic/context observations')
for i,value in enumerate(values): print(i,value)
