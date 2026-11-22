from pathlib import Path
import json,hashlib,subprocess
OUT=Path(__file__).resolve().parent
jars=[Path('/home/bobby/.m2/repository')/p for p in ['org/clojure/clojure/1.12.6/clojure-1.12.6.jar','org/clojure/spec.alpha/0.5.238/spec.alpha-0.5.238.jar','org/clojure/core.specs.alpha/0.4.74/core.specs.alpha-0.4.74.jar']]
cases=[];jvm=[]
payloads=['0','-0','+0010','010','-010','9223372036854775808','-18446744073709551616']
for payload in payloads:
 literal='#suss/biginteger '+json.dumps(payload)
 expected=f'(biginteger {json.dumps(payload)})'
 for reader in ['read-string','edn/read-string']:
  cases.append(f'(let [x ({reader} {json.dumps(literal)})] [(pr-str x) (pr-str (class x)) (hash x) (= x (bigint x))])')
  jvm.append(f'(let [x {expected}] [(pr-str x) (pr-str (class x)) (hash x) (= x (bigint x))])')
 cases.append(f'(let [biginteger (fn [x] :shadow) x {literal}] [(pr-str x) (pr-str (class x))])')
 jvm.append(f'(let [x {expected}] [(pr-str x) (pr-str (class x))])')
encoded=json.dumps('#suss/biginteger "1"')
cases += [f"(binding [*data-readers* {{'suss/biginteger (fn [x] :custom)}}] (read-string {encoded}))", f"(edn/read-string {{:readers {{'suss/biginteger (fn [x] :custom)}}}} {encoded})"]
jvm += cases[-2:]
source="(require '[clojure.edn :as edn])\n"+'\n'.join('(prn '+s+')' for s in jvm)
command=['java','-cp',':'.join(map(str,jars)),'clojure.main','-e',source]
r=subprocess.run(command,text=True,capture_output=True)
(OUT/'carrier-oracle.json').write_text(json.dumps({'command':command,'jars':{str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in jars},'exit_code':r.returncode,'stdout':r.stdout,'stderr':r.stderr},indent=2)+'\n');assert r.returncode==0,r.stderr
values=r.stdout.splitlines();assert len(values)==len(cases)
(OUT/'carrier-cases.json').write_text(json.dumps(list(zip(cases,values)),indent=2)+'\n')
print('fresh JVM carrier value/category controls',len(cases))
