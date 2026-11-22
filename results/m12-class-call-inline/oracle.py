from pathlib import Path
import hashlib,json,subprocess
OUT=Path(__file__).resolve().parent
jars=[Path('/home/bobby/.m2/repository')/p for p in ['org/clojure/clojure/1.12.6/clojure-1.12.6.jar','org/clojure/spec.alpha/0.5.238/spec.alpha-0.5.238.jar','org/clojure/core.specs.alpha/0.4.74/core.specs.alpha-0.4.74.jar']]
source='(let [c (type 7) a (try (c) (catch Throwable e e)) b (try (c) (catch Throwable e e))] [(identical? a b) (nil? (ex-data a)) (nil? (ex-data b))])'
command=['java','-cp',':'.join(map(str,jars)),'clojure.main','-e','(prn '+source+')']
r=subprocess.run(command,text=True,capture_output=True)
(OUT/'oracle.json').write_text(json.dumps({'command':command,'jars':{str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in jars},'exit_code':r.returncode,'stdout':r.stdout,'stderr':r.stderr},indent=2)+'\n')
assert r.returncode==0,r.stderr
rows=[{'source':source.replace('(catch Throwable','(catch :default'),'expected':r.stdout.strip()}]
(OUT/'cases.json').write_text(json.dumps(rows,indent=2)+'\n')
print(r.stdout,end='')
