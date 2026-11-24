"""Execute pinned JVM char casts before implementing their source overloads."""
from pathlib import Path
import hashlib,json,subprocess,time,re
BASE=Path(__file__).resolve().parent;OUT=BASE/'primary';OUT.mkdir(exist_ok=True)
assert not (OUT/'command.json').exists()
values=['0','65','65535','-1','65536','65.9','-0.9','65535.9','##NaN','##Inf','##-Inf','65N','18446744073709551681N','-18446744073709551616N','18446744073709617152N','131/2','-1/2','655359/10','65.9M','-0.9M','65535.9M',r'\a','(char 55296)','nil','true','"a"','"ab"',':x']
rows=[]
for value in values:
 for route,expression in [('direct',f'(char {value})'),('var',f'((identity char) {value})'),('apply',f'(apply char [{value}])')]:
  rows.append({'name':route+'-'+str(len(rows)),'route':route,'input':value,'expression':expression})
for expression in ['(let [x -0.9] (char x))','(let [x (identity -0.9)] (char x))','((fn [x] (char x)) -0.9)','((fn [^double x] (char x)) -0.9)','((fn [^double x] (char x)) ##NaN)','((fn [x] (char x)) ##NaN)','(let [^double x (identity -0.9)] (char x))','(char (identity 65535.9))','(char (double -0.9))','(char (float -0.9))']:
 rows.append({'name':'source-kind-'+str(len(rows)),'route':'source-kind','expression':expression})
source=OUT/'oracle.clj';source.write_text('(assert (= "1.12.6" (clojure-version)))\n'+''.join('(println (pr-str (pr-str (try [:ok (int '+row['expression']+')] (catch Throwable e [:throws (.getName (class e)) (.getMessage e)])))))\n' for row in rows))
jars=[Path.home()/('.m2/repository/org/clojure/'+p) for p in ['clojure/1.12.6/clojure-1.12.6.jar','spec.alpha/0.5.238/spec.alpha-0.5.238.jar','core.specs.alpha/0.4.74/core.specs.alpha-0.4.74.jar']]
command=['java','-Xmx512m','-cp',':'.join(map(str,jars)),'clojure.main',str(source)]
started=time.time();r=subprocess.run(command,capture_output=True);(OUT/'stdout').write_bytes(r.stdout);(OUT/'stderr').write_bytes(r.stderr)
record={'command':command,'exit_code':r.returncode,'seconds':time.time()-started,'source_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'jars':{str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in jars},'stdout_sha256':hashlib.sha256(r.stdout).hexdigest()};(OUT/'command.json').write_text(json.dumps(record,indent=2)+'\n')
assert r.returncode==0,r.stderr.decode()
lines=[json.loads(line) for line in r.stdout.decode().splitlines()];assert len(lines)==len(rows)
for row,line in zip(rows,lines):
 row['primary_edn']=line
 match=re.fullmatch(r'\[:ok (\d+)\]',line)
 if match:row['unit']=int(match[1])
 else:
  assert line.startswith('[:throws '),line
  decoder=json.JSONDecoder();class_name,end=decoder.raw_decode(line[len('[:throws '):]);message=line[len('[:throws ')+end:].strip()[:-1];row['error_class']=class_name;row['message']=None if message=='nil' else json.loads(message)
(OUT/'cases.json').write_text(json.dumps(rows,indent=2)+'\n');print('fresh JVM cases',len(rows))
for row in rows:
 if row['route']=='source-kind':print(row['expression'],row['primary_edn'])
