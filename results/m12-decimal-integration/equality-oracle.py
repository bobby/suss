"""Fresh category/equality/hash/order/case observations for Decimal dispatch."""
from pathlib import Path
import hashlib,json,subprocess,time
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
expressions=[
 '[(= 1M 1.0M) (= 1M 1.00M) (= 1M 1) (= 1M 1N) (= 1M 1.0) (= 0M 0.00M)]',
 '[(== 1M 1) (== 1M 1N) (== 1M 1.0) (== 0.5M 1/2) (== 1M 1/3)]',
 '[(hash 0M) (hash 0.00M) (hash 1M) (hash 1.00M) (hash 1.50M) (hash -1.50M) (hash 123456789012345678901234567890.00M)]',
 '[(= 9007199254740992M 9007199254740993M) (< 9007199254740992M 9007199254740993M) (compare 9007199254740993M 9007199254740992M)]',
 '[(compare 1.50M 1) (compare 1 1.50M) (compare -1.50M -1N) (compare 1.50M 1.5)]',
 '[(compare 0.50M 1/2) (< 0.25M 1/2) (> 1/2 0.25M)]',
 '(compare 1M 1/3)',
 '(with-precision 3 [(== 1/3 0.333M) (= 1/3 0.333M) (compare 1/3 0.333M) (compare 0.333M 1/3)])',
 '(with-precision 2 [(= 1.234M 1.23M) (compare 1.234M 1.23M) (== 1.234M 1.23M) (hash 1.234M)])',
 '[(case 1.00M 1.0M :same :different) (case 1.00M 1.00M :same :different) (case 1M 1 :same :different)]',
 '[(case 1.0M 1M :a 2M :b :none) (case 1M 1.0M :a 2M :b :none)]',
 '[(compare 1E+2147483648M 1M) (compare 1E-2147483647M 0M) (compare -1E-2147483647M 0M) (= 0E+2147483648M 0E-2147483647M)]',
 '[(hash 0E+2147483648M) (hash 1E+2147483648M) (hash 1E-2147483647M)]',
 '(hash (bigdec "10E+2147483648"))',
 '[(= 1.00000000000000000000000000000000000001M 1M) (> 1.00000000000000000000000000000000000001M 1M)]',
 '(count (hash-set 1M 1.0M 1.00M))',
 '(get (hash-map 1.00M :decimal 1 :integer 1.0 :double) 1.0M)',
 '(let [events (atom [])] [(< (do (swap! events conj 1) 1M) (do (swap! events conj 2) 2M)) @events])',
 '[(pos? 1E-2147483647M) (neg? -1E-2147483647M) (zero? 1E-2147483647M) (zero? 0.00M)]',
]
source='(assert (= "1.12.6" (clojure-version)))\n'+''.join('(println (pr-str (pr-str (try '+e+' (catch Throwable e [:error (.getMessage e)])))))\n' for e in expressions)
(OUT/'equality-oracle.clj').write_text(source)
prior=json.loads((ROOT/'results/m12-wide-wit/oracle-command.json').read_text());command=prior['command'][:-1]+[str(OUT/'equality-oracle.clj')]
t=time.time();r=subprocess.run(command,cwd=ROOT,text=True,capture_output=True)
(OUT/'equality-oracle.log').write_text(r.stdout+r.stderr)
(OUT/'equality-oracle-command.json').write_text(json.dumps({'command':command,'exit_code':r.returncode,'seconds':time.time()-t,
 'source_sha256':hashlib.sha256(source.encode()).hexdigest(),'jars':{p:hashlib.sha256(Path(p).read_bytes()).hexdigest() for p in prior['jars']}},indent=2)+'\n')
assert r.returncode==0,r.stderr
values=[json.loads(x) for x in r.stdout.splitlines()];assert len(values)==len(expressions)
(OUT/'equality-cases.json').write_text(json.dumps(list(zip(expressions,values)),indent=2)+'\n')
for i,x in enumerate(values):print(i,x)
