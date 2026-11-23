from pathlib import Path
import hashlib,json,subprocess,time
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
expressions=[]
for n in [31,32,33,63,64,65,127,128,129,257]:
 values='['+' '.join(f'[{(i*37)%17-8} {i}]' for i in range(n))+']'
 expressions.append('(sort (fn [[a _] [b _]] (compare a b)) '+values+')')
for keys in [list(range(128)),list(range(127,-1,-1)),[i if i%2 else -i for i in range(128)],[0]*128]:
 values='['+' '.join(f'[{k} {i}]' for i,k in enumerate(keys))+']'
 expressions.append('(sort (fn [[a _] [b _]] (compare a b)) '+values+')')
source='(assert (= "1.12.6" (clojure-version)))\n'+''.join('(println (pr-str (pr-str (try '+e+' (catch Throwable e [:error (.getMessage e)])))))\n' for e in expressions)
(OUT/'sort-large-oracle.clj').write_text(source)
prior=json.loads((ROOT/'results/m12-wide-wit/oracle-command.json').read_text());command=prior['command'][:-1]+[str(OUT/'sort-large-oracle.clj')]
t=time.time();r=subprocess.run(command,cwd=ROOT,text=True,capture_output=True)
(OUT/'sort-large-oracle.log').write_text(r.stdout+r.stderr);(OUT/'sort-large-oracle-command.json').write_text(json.dumps({'command':command,'exit_code':r.returncode,'seconds':time.time()-t,'source_sha256':hashlib.sha256(source.encode()).hexdigest(),'jars':{p:hashlib.sha256(Path(p).read_bytes()).hexdigest() for p in prior['jars']}},indent=2)+'\n')
assert r.returncode==0,r.stderr
values=[json.loads(x) for x in r.stdout.splitlines()];assert len(values)==len(expressions) and all(not x.startswith('[:error') for x in values)
(OUT/'sort-large-cases.json').write_text(json.dumps(list(zip(expressions,values)),indent=2)+'\n');print(len(values),'large stable ordering controls')
