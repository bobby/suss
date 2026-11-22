from pathlib import Path
import hashlib,json,subprocess,time
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
expressions=[
 '[1M 1.50M -0.00M 0E+20M 1E-6M 1E-7M 1.2300E+8M 123456789012345678901234567890.123400M]',
 '[(str 1.50M) (str -0.00M) (str 0E-100M) (str 1E+2147483647M) (str 1E-2147483647M)]',
 '[(number? 1M) (decimal? 1M) (decimal? 1) (decimal? 1N) (decimal? 1/2) (decimal? 1.0) (integer? 1M) (rational? 1M)]',
 '[(= 1M 1.0M) (= 1M 1) (= 1M 1N) (= 1M 1.0) (= 0M 0.00M) (== 1M 1N) (compare 1M 1/2)]',
 '[(hash 0M) (hash 0.00M) (hash 1M) (hash 1.00M) (hash 1.50M) (hash -1.50M) (hash 123456789012345678901234567890.00M)]',
 '[(+ 1.50M 2) (- 1.50M 2N) (* 1.5M 2) (/ 1M 8) (+ 0.00M 1M) (* 0.00M 1E+20M)]',
 '[(quot 1.50M 0.4M) (rem 1.50M 0.4M) (mod -1.50M 0.4M) (abs -1.50M) (- 1.50M) (inc 1.50M)]',
 '[(+ 1.50M 0.5) (+ 1.50M 1/2) (* 1.50M 1/2) (/ 1.50M 3/2)]',
 '(/ 1M 3)', '(+ 1M 1/3)', '(== 1M 1/3)', '(compare 1M 1/3)',
 '(with-precision 3 [(/ 1M 3) (+ 1.2345M 0M) (* 1.2345M 2M) (str *math-context*)])',
 '(with-precision 3 [(/ 1M 3) (== 1/3 0.333M) (compare 1/3 0.333M) (+ 1/3 0M)])',
 '[(bigdec 1) (bigdec 1N) (bigdec 1.25) (bigdec "1.2300") (bigdec 1/8) (bigdec 1.50M) (bigint 1.50M) (biginteger -1.50M)]',
 '(bigdec 1/3)', '(bigdec nil)', '(bigdec ##NaN)', '(bigdec ##Inf)',
 '[(double 1.50M) (float 1.50M) (long 1.50M) (int -1.50M) (unchecked-long 18446744073709551617M)]',
 '(long 18446744073709551617M)', '(int 4294967297M)',
 '[(read-string "1.50M") (clojure.edn/read-string "1.50M") (read-string "1e+3M")]',
 '(with-precision 0 (/ 1M 3))', '(with-precision -1 1M)',
 '(with-precision 2 [(+ 999M 1M) (bigdec "1.2345") (bigdec 1/7) (str *math-context*)])',
 '(let [x *math-context*] (try (with-precision 2 (/ 1M 0)) (catch Throwable _)) (identical? x *math-context*))',
 '(case 1.00M 1.0M :same :different)',
 '[(= 1M 1.000M) (hash 1E+20M) (hash 1E-20M) (compare 1E+2147483647M 1M) (compare -1E-2147483647M 0M)]',
]
for mode in ['UP','DOWN','CEILING','FLOOR','HALF_UP','HALF_DOWN','HALF_EVEN','UNNECESSARY']:
 expressions.append(f'(with-precision 2 :rounding {mode} [(+ 1.25M 0M) (+ -1.25M 0M) (+ 1.35M 0M) (/ 1M 8)])')
for text in ['1E+2147483648M','1E-2147483648M','1.0E+2147483647M']:
 expressions.append('(read-string '+json.dumps(text)+')')
source='(require (quote clojure.edn))\n(assert (= "1.12.6" (clojure-version)))\n'+'\n'.join('(println (pr-str (pr-str (try '+e+' (catch Throwable e [:error (.getMessage e)])))))' for e in expressions)+'\n'
(OUT/'oracle.clj').write_text(source)
prior=json.loads((ROOT/'results/m12-wide-wit/oracle-command.json').read_text());command=prior['command'][:-1]+[str(OUT/'oracle.clj')]
start=time.time();r=subprocess.run(command,cwd=ROOT,capture_output=True,text=True);(OUT/'oracle.log').write_text(r.stdout+r.stderr)
(OUT/'oracle-command.json').write_text(json.dumps({'command':command,'exit_code':r.returncode,'seconds':time.time()-start,'source_sha256':hashlib.sha256(source.encode()).hexdigest(),'jars':{p:hashlib.sha256(Path(p).read_bytes()).hexdigest() for p in prior['jars']}},indent=2)+'\n')
assert r.returncode==0,r.stderr
values=[json.loads(x) for x in r.stdout.splitlines()];assert len(values)==len(expressions)
(OUT/'cases.json').write_text(json.dumps(list(zip(expressions,values)),indent=2)+'\n');print(len(values),'fresh Clojure decimal observations')
for i,value in enumerate(values):print(i,value)
