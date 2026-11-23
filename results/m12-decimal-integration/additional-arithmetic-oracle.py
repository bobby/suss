from pathlib import Path
import hashlib,json,subprocess,time
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
expressions=[
 '(with-precision 2 (pr-str [(- 1M 1.234M) (- 1.0 1.234M) (- 1.234M 1M)]))',
 "(with-precision 2 (pr-str [(-' 1M 1.234M) (unchecked-subtract 1M 1.234M)]))",
 '(with-precision 2 (pr-str [(unchecked-negate 1.234M) (unchecked-inc 1.234M) (unchecked-dec 1.234M)]))',
 "(with-precision 2 (pr-str [(+ 1.234M) (+' 1.234M) (* 1.234M) (*' 1.234M)]))",
 '(with-precision 2 (pr-str [(apply - [1M 1.234M]) ((fn [f] (f 1M 1.234M)) -)]))',
 '(with-precision 3 (pr-str [(+ 1M 1/3) (* 1.234M 1/3) (/ 1/3 1M)]))',
 '(with-precision 1 (pr-str [(+ 1/3 1M) (* 1M 1/3) (/ 1M 1/3)]))',
 '(with-precision 2 (pr-str [(min 1.234M 1.235M) (max 1.234M 1.235M)]))',
 '(with-precision 2 (let [events (atom [])] [(pr-str (- (do (swap! events conj :left) 1M) (do (swap! events conj :right) 1.234M))) @events]))',
 '(with-precision 2 (pr-str [(+ 9.94M 0.06M) (+ 9.95M 0.05M) (- 9.95M 0.05M)]))',
 '(with-precision 3 (pr-str [(/ 1M -7M) (/ -1M -7M) (/ 999M 0.01M)]))',
 '(with-precision 3 (pr-str [(quot 1E+5M 2M) (rem 1E+5M 2M) (quot 1.00M 3M) (rem 1.00M 3M)]))',
 '(with-precision 3 (pr-str (+ 1E+2147483648M 1M)))',
 '(with-precision 3 (pr-str (+ 1E+1000000M 1M)))',
 '(with-precision 3 (pr-str (- 1E+1000000M 1M)))',
 '(with-precision 3 :rounding FLOOR (pr-str (- 1E+1000000M 1M)))',
 '(with-precision 3 :rounding UP (pr-str (+ 1E+1000000M 1M)))',
 '(with-precision 3 :rounding UNNECESSARY (pr-str (+ 1E+1000000M 1M)))',
 '(let [n (atom 0)] (with-precision (do (swap! n inc) 3) [(str *math-context*) @n]))',
 '(let [c (with-precision 3 *math-context*)] (binding [*math-context* c] (pr-str (/ 1M 7))))',
 '(let [saved (with-precision 3 (fn [] (pr-str (/ 1M 7))))] (with-precision 2 (saved)))',
 '(with-precision 2 (pr-str [(+ 1.230M -1.230M) (* 0.000M -1E+20M) (/ 0.000M -1E+20M)]))',
]
source='(assert (= "1.12.6" (clojure-version)))\n'+''.join('(println (pr-str (pr-str (try '+e+' (catch Throwable e [:error (.getMessage e)])))))\n' for e in expressions)
(OUT/'additional-arithmetic-oracle.clj').write_text(source)
prior=json.loads((ROOT/'results/m12-wide-wit/oracle-command.json').read_text());command=prior['command'][:-1]+[str(OUT/'additional-arithmetic-oracle.clj')];command.insert(1,'-Xmx512m')
t=time.time();r=subprocess.run(command,cwd=ROOT,text=True,capture_output=True)
(OUT/'additional-arithmetic-oracle.log').write_text(r.stdout+r.stderr)
(OUT/'additional-arithmetic-oracle-command.json').write_text(json.dumps({'command':command,'exit_code':r.returncode,'seconds':time.time()-t,'source_sha256':hashlib.sha256(source.encode()).hexdigest(),'jars':{p:hashlib.sha256(Path(p).read_bytes()).hexdigest() for p in prior['jars']}},indent=2)+'\n')
assert r.returncode==0,r.stderr
values=[json.loads(x) for x in r.stdout.splitlines()];assert len(values)==len(expressions)
(OUT/'additional-arithmetic-cases.json').write_text(json.dumps(list(zip(expressions,values)),indent=2)+'\n')
for i,x in enumerate(values):print(i,x)
