from pathlib import Path
import hashlib,json,subprocess
OUT=Path(__file__).resolve().parent
jars=[Path('/home/bobby/.m2/repository')/p for p in ['org/clojure/clojure/1.12.6/clojure-1.12.6.jar','org/clojure/spec.alpha/0.5.238/spec.alpha-0.5.238.jar','org/clojure/core.specs.alpha/0.4.74/core.specs.alpha-0.4.74.jar']]
cases=['[#_ {1N :a 2N :b} :kept]','[#_ #{1N 2N} :kept]','[#_ {1/2 :a 2/3 :b} :kept]','1N','-0N','9223372036854775808','-9223372036854775809','16rFFFFFFFFFFFFFFFF','0x10000000000000000','2/4','-2/4','4/2','0/5','[1N 2/4 18446744073709551616N]','{:a 18446744073709551616N}','(1N 2/3 -7N)','[1/123456789012345678901234567890 123456789012345678901234567890/7]']
source="(require '[clojure.edn :as edn])\n"+'\n'.join('(prn (edn/read-string '+json.dumps(s)+'))' for s in cases)
command=['java','-cp',':'.join(map(str,jars)),'clojure.main','-e',source]
r=subprocess.run(command,text=True,capture_output=True)
(OUT/'oracle.json').write_text(json.dumps({'command':command,'jars':{str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in jars},'exit_code':r.returncode,'stdout':r.stdout,'stderr':r.stderr},indent=2)+'\n')
assert r.returncode==0,r.stderr
values=r.stdout.splitlines();assert len(values)==len(cases)
(OUT/'cases.tsv').write_text(''.join(s+'\t'+v+'\n' for s,v in zip(cases,values)))
print('fresh JVM EDN cases',len(cases))
