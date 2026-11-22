from pathlib import Path
import subprocess,json,hashlib,time
OUT=Path(__file__).resolve().parent
jars=[Path('/home/bobby/.m2/repository')/p for p in ['org/clojure/clojure/1.12.6/clojure-1.12.6.jar','org/clojure/spec.alpha/0.5.238/spec.alpha-0.5.238.jar','org/clojure/core.specs.alpha/0.4.74/core.specs.alpha-0.4.74.jar']]
samples=[('7','java.lang.Long'),('7.0','java.lang.Double'),('7N','clojure.lang.BigInt'),('(biginteger 7)','java.math.BigInteger'),('1/3','clojure.lang.Ratio'),('99999999999999999999999N','clojure.lang.BigInt'),('-2/7','clojure.lang.Ratio'),('(numerator 2/7)','java.math.BigInteger'),('(denominator 2/7)','java.math.BigInteger')]
cases=[]
for expr,cls in samples:
 cases.append(f'(let [x {expr} c (class x)] [(str c) (pr-str c) (= c (type x)) (instance? {cls} x) (instance? java.lang.Number x) (instance? java.lang.Class c) (fn? c) (ifn? c) (meta c) (get {{c :found}} c)])')
for _,cls in samples[:5]:
 cases.append('['+' '.join(f'(instance? {cls} {expr})' for expr,_ in samples[:5])+']')
cases += [
 '[(= (type (type 1N)) java.lang.Class) (= (class java.lang.Class) java.lang.Class) (instance? java.lang.Class java.lang.Class)]',
 '(let [c (class 1N) f (fn [x] (instance? clojure.lang.BigInt x))] [(f 1) (f 1N) (f 1/3) (f nil) (f (biginteger 1)) (identical? c (class (+ 1N 2N)))])',
 '(let [c (class 1N)] (try (c) (catch Throwable e :not-callable)))',
 '[(instance? java.lang.Number nil) (instance? java.lang.Number "7") (instance? java.lang.Number []) (instance? java.lang.Class 7) (instance? java.lang.Class nil)]',
 '[(str java.math.BigDecimal) (pr-str java.math.BigDecimal) (instance? java.lang.Class java.math.BigDecimal)]',
 '[(pr-str Long) (pr-str Double) (pr-str BigInteger) (pr-str BigDecimal) (pr-str Number) (pr-str Class)]',
]
source='\n'.join('(prn '+s+')' for s in cases)
command=['java','-cp',':'.join(str(p) for p in jars),'clojure.main','-e',source]
start=time.time();p=subprocess.run(command,text=True,capture_output=True)
(OUT/'oracle.clj').write_text(source+'\n');(OUT/'jvm.stdout').write_text(p.stdout);(OUT/'jvm.stderr').write_text(p.stderr)
assert p.returncode==0,p.stderr
expected=p.stdout.splitlines();assert len(expected)==len(cases),(len(expected),len(cases))
rows=[{'source':s.replace('(catch Throwable','(catch :default'),'expected':e} for s,e in zip(cases,expected)]
(OUT/'cases.json').write_text(json.dumps(rows,indent=2)+'\n')
(OUT/'oracle.json').write_text(json.dumps({'command':command,'jars':{str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in jars},'seconds':time.time()-start,'exit_code':p.returncode,'cases':len(cases)},indent=2)+'\n')
print('JVM numeric class cases:',len(cases),p.stdout,flush=True)
