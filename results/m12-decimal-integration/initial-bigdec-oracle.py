from pathlib import Path
import hashlib,json,subprocess,time
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
expressions=[
 '(pr-str (mapv bigdec [0 1 -1 1N -1N 9223372036854775808N 1/2 -1/2 0.1 -0.0 1.50M])))',
 '(pr-str (bigdec (biginteger "123456789012345678901234567890")))',
 '(pr-str (mapv bigdec ["1.50" "-0.00" "010" "+.5e+2" ".5" "1." "1e-5"])))',
 '(bigdec 1/3)',
 '(with-precision 3 (pr-str (bigdec 1/3)))',
 '(with-precision 2 :rounding DOWN (pr-str (bigdec -1/3)))',
 '(with-precision 2 (pr-str [(bigdec "1.23456") (bigdec 1.23456M) (bigdec 1.23456)]))',
 '(pr-str [(bigdec 4.9e-324) (bigdec 1.7976931348623157e308)])',
 '(pr-str [(bigdec "1E2147483648") (bigdec "1E-2147483647") (bigdec "100E+2147483648")])',
 '(mapv (fn [x] (try (bigdec x) false (catch Throwable _ true))) [nil [] {} true false :x "abc" "1M" " 1" "1 " ##Inf ##NaN])',
 '(mapv (fn [x] (try (bigdec x) false (catch Throwable _ true))) ["" "." "-" "-." "1e" "1E-2147483648" "1E2147483649" "0x10"])',
 '(let [trace (atom [])] [(pr-str (bigdec (do (swap! trace conj :value) "1.50"))) @trace])',
 '(let [f bigdec] (pr-str [(f 1/2) (f "1.50") (f 9007199254740993N)]))',
 '(hash (bigdec "10E+2147483648"))',
]
source='(assert (= "1.12.6" (clojure-version)))\n'+''.join('(println (pr-str (pr-str (try '+e+' (catch Throwable e [:error (.getMessage e)])))))\n' for e in expressions)
(OUT/'bigdec-oracle.clj').write_text(source)
prior=json.loads((ROOT/'results/m12-wide-wit/oracle-command.json').read_text());command=prior['command'][:-1]+[str(OUT/'bigdec-oracle.clj')]
t=time.time();r=subprocess.run(command,cwd=ROOT,text=True,capture_output=True)
(OUT/'bigdec-oracle.log').write_text(r.stdout+r.stderr)
(OUT/'bigdec-oracle-command.json').write_text(json.dumps({'command':command,'exit_code':r.returncode,'seconds':time.time()-t,'source_sha256':hashlib.sha256(source.encode()).hexdigest(),'jars':{p:hashlib.sha256(Path(p).read_bytes()).hexdigest() for p in prior['jars']}},indent=2)+'\n')
assert r.returncode==0,r.stderr
values=[json.loads(x) for x in r.stdout.splitlines()];assert len(values)==len(expressions)
(OUT/'bigdec-cases.json').write_text(json.dumps(list(zip(expressions,values)),indent=2)+'\n')
for i,x in enumerate(values):print(i,x)
