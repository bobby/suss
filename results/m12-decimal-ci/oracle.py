from pathlib import Path
import subprocess,json,hashlib
OUT=Path(__file__).resolve().parent
jars=[Path('/home/bobby/.m2/repository')/p for p in ['org/clojure/clojure/1.12.6/clojure-1.12.6.jar','org/clojure/spec.alpha/0.5.238/spec.alpha-0.5.238.jar','org/clojure/core.specs.alpha/0.4.74/core.specs.alpha-0.4.74.jar']]
rows=[{'setup':'','source':s} for s in [
 '(pr-str 1M)', '(pr-str 1.50M)', '(pr-str -0.00M)',
 '(pr-str [1.50M -0.00M 123456789012345678901234567890.00M])',
 '(let [x (read-string "1.50M")] [(pr-str x) (decimal? x) (rational? x) (number? x) (= (class x) BigDecimal)])',
 '(let [x 1.50M] [(str (class x)) (pr-str (class x)) (instance? BigDecimal x) (instance? Number x) (ifn? (class x))])',
 '(let [c (class 1M) trace (atom 0)] [(try (c (do (swap! trace inc) 7)) false (catch Throwable _ true)) @trace])',
 '((fn [x] [(pr-str x) (decimal? x) (= (type x) BigDecimal)]) 1.25M)',
]]
rows.append({'setup':'(def ^:const decimal-ci-constant 1.50M) (def decimal-ci-read (fn [] decimal-ci-constant)) (def decimal-ci-constant 2.50M)','source':'[(pr-str (decimal-ci-read)) (pr-str decimal-ci-constant)]'})
rows.append({'setup':'(defprotocol DecimalCiProtocol (decimal-ci-kind [x])) (extend-type java.math.BigDecimal DecimalCiProtocol (decimal-ci-kind [x] :decimal)) (extend-type java.lang.Number DecimalCiProtocol (decimal-ci-kind [x] :number))','source':'[(decimal-ci-kind 1M) (decimal-ci-kind 1) (decimal-ci-kind 1N) (decimal-ci-kind 1/3)]'})
# Replay the existing constructor-derived formats through actual read-string.
for item in json.loads((OUT.parent/'m12-decimal-values/format-cases.json').read_text()):
 text=item['unscaled']+'E'+str(-item['scale'])+'M'
 expr='(let [x (read-string '+json.dumps(text)+')] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)])'
 rows.append({'setup':'','source':expr})
source='\n'.join(r['setup']+'\n(prn '+r['source']+')' for r in rows)
(OUT/'oracle.clj').write_text(source+'\n')
command=['java','-cp',':'.join(map(str,jars)),'clojure.main',str(OUT/'oracle.clj')]
r=subprocess.run(command,text=True,capture_output=True)
(OUT/'oracle.json').write_text(json.dumps({'command':command,'source_sha256':hashlib.sha256((OUT/'oracle.clj').read_bytes()).hexdigest(),'jars':{str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in jars},'exit_code':r.returncode,'stdout':r.stdout,'stderr':r.stderr},indent=2)+'\n')
assert r.returncode==0,r.stderr
values=r.stdout.splitlines();assert len(values)==len(rows),(values,rows)
for row,value in zip(rows,values):
 row['expected']=value
 row['source']=row['source'].replace('(catch Throwable','(catch :default')
(OUT/'cases.json').write_text(json.dumps(rows,indent=2)+'\n')
print('fresh JVM Decimal value/class cases',len(rows))
