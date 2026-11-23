from pathlib import Path
import hashlib,json,subprocess,time
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
values=['nil','false','true','0','1','-1','9223372036854775807','-9223372036854775808','0N','1N','-1N','(biginteger 1)','(biginteger -1)','1267650600228229401496703205376N','0.0','1.0','-1.0','0M','1.0M','-1.0M','1/3','-1/3','##NaN','##Inf','##-Inf',':x',"'x",'[]','[1]','{}','{:a 1}',"'()","'(1)",'#{}','#{1}','(seq [1])','(range 2)','(sorted-map :a 1)','(sorted-set 1)','(first {:a 1})','(fn [x] x)']
predicates=['pos-int?','neg-int?','nat-int?','coll?','sorted?','char?']
expressions=[f'({p} {v})' for p in predicates for v in values]
expressions += [f'(char? {v})' for v in ['\\a','\\space','\\newline','\\u0000','(char 55296)','\\uFFFF','""','"ab"','"😀"']]
# Once-only arguments and calls through first-class aliases/apply are observable.
for p in predicates:
 expressions += [f'(let [f {p}] (f 1M))',f'(apply {p} [1M])',f'(do (def predicate-effects []) (let [r ({p} (do (set! predicate-effects (conj predicate-effects :arg)) 1M))] [r predicate-effects]))']
source='(assert (= "1.12.6" (clojure-version)))\n'+''.join('(println (pr-str (pr-str (try '+e+' (catch Throwable e [:error (.getMessage e)])))))\n' for e in expressions)
(OUT/'oracle.clj').write_text(source)
prior=json.loads((ROOT/'results/m12-wide-wit/oracle-command.json').read_text());command=prior['command'][:-1]+[str(OUT/'oracle.clj')]
t=time.time();r=subprocess.run(command,cwd=ROOT,text=True,capture_output=True)
(OUT/'oracle.log').write_text(r.stdout+r.stderr)
(OUT/'oracle-command.json').write_text(json.dumps({'command':command,'exit_code':r.returncode,'seconds':time.time()-t,'source_sha256':hashlib.sha256(source.encode()).hexdigest(),'jars':{p:hashlib.sha256(Path(p).read_bytes()).hexdigest() for p in prior['jars']}},indent=2)+'\n')
assert r.returncode==0,r.stderr
values=[json.loads(x) for x in r.stdout.splitlines()];assert len(values)==len(expressions)
assert all(not v.startswith('[:error') for v in values)
(OUT/'cases.json').write_text(json.dumps(list(zip(expressions,values)),indent=2)+'\n')
print(len(expressions),'fresh JVM controls')
