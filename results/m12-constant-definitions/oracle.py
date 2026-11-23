"""Check retained constant cases against fresh JVM Clojure1.12.6 processes."""
from pathlib import Path
import subprocess,json,hashlib,time
OUT=Path(__file__).resolve().parent
jars=[Path('/home/bobby/.m2/repository')/p for p in ['org/clojure/clojure/1.12.6/clojure-1.12.6.jar','org/clojure/spec.alpha/0.5.238/spec.alpha-0.5.238.jar','org/clojure/core.specs.alpha/0.4.74/core.specs.alpha-0.4.74.jar']]
rows=json.loads((OUT/'cases.json').read_text());results=[]
for i,row in enumerate(rows):
 # The only syntax adaptation is the native Throwable catch selector.
 source=row['source'].replace('(catch :default','(catch Throwable')
 command=['java','-cp',':'.join(str(p) for p in jars),'clojure.main','-e',source]
 start=time.time();p=subprocess.run(command,text=True,capture_output=True)
 result={'case':i,'command':command,'exit_code':p.returncode,'seconds':time.time()-start,'stdout':p.stdout,'stderr':p.stderr}
 results.append(result)
 assert p.returncode==0 and p.stdout.splitlines()[-1]==row['expected'],result
record={'clojure':'1.12.6','cases_sha256':hashlib.sha256((OUT/'cases.json').read_bytes()).hexdigest(),'jars':{str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in jars},'commands':results,'complete':True}
(OUT/'oracle-check.json').write_text(json.dumps(record,indent=2)+'\n')
print('fresh JVM matches:',len(rows),flush=True)
