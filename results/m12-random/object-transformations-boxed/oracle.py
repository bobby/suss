"""Match the Object overload visible in retained rand/rand-int JVM bytecode."""
from pathlib import Path
import hashlib,json,subprocess,time
BASE=Path(__file__).resolve().parent
p=json.loads((BASE.parent/'source-provenance.json').read_text());jars=p['jars'];sha=lambda b:hashlib.sha256(b).hexdigest()
assert all(sha(Path(path).read_bytes())==value for path,value in jars.items())
old=json.loads((BASE.parent/'controlled-transformations.json').read_text())
rows=[]
for row in old:
 product='(* bound draw)'
 body='(int product)' if row['operation']=='rand-int' else 'product'
 # Identity calls box primitive initializers; explicit Object locals select the actual public overloads.
 expression='(try (let [^Object bound (identity '+row['bound']+') ^Object draw (identity '+row['draw']+') ^Object product '+product+' x '+body+'] [:value (pr-str x) (cond (double? x) :double (integer? x) :integer :else :other)]) (catch Throwable e [:error]))'
 rows.append({k:row[k] for k in ['operation','bound','draw']}|{'expression':expression})
source='(assert (= "1.12.6" (clojure-version)))\n'+''.join('(println (pr-str (pr-str '+r['expression']+')))\n' for r in rows)
(BASE/'oracle.clj').write_text(source)
command=['java','-Xmx512m','-cp',':'.join(jars),'clojure.main',str(BASE/'oracle.clj')]
start=time.time();r=subprocess.run(command,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
(BASE/'stdout.log').write_bytes(r.stdout);(BASE/'stderr.log').write_bytes(r.stderr)
record={'command':command,'exit_code':r.returncode,'seconds':time.time()-start,'jars':jars,'source_sha256':sha(source.encode()),'stdout_sha256':sha(r.stdout),'original_fixture_sha256':sha((BASE.parent/'controlled-transformations.json').read_bytes()),'controls':len(rows),'qualification':'Retained rand bytecode calls Numbers.multiply(Object,Object), returning Object; rand-int then calls RT.intCast(Object). Identity calls box initializers and explicit Object locals reproduce these overloads. The first hint-only probe fails compilation and remains separately retained. The earlier direct-literal transformations and their native comparison failure remain unchanged. This primary recording alone is not a native pass.'}
(BASE/'command.json').write_text(json.dumps(record,indent=2)+'\n')
assert r.returncode==0,r.stderr.decode()
values=[json.loads(line) for line in r.stdout.decode().splitlines()];assert len(values)==len(rows)==126
for row,value in zip(rows,values):row['expected']=value
(BASE/'cases.json').write_text(json.dumps(rows,indent=2)+'\n')
diffs=[{'operation':a['operation'],'bound':a['bound'],'draw':a['draw'],'literal_expected':a['expected'],'object_expected':b['expected']} for a,b in zip(old,rows) if a['expected']!=b['expected']]
(BASE/'overload-differences.json').write_text(json.dumps(diffs,indent=2)+'\n')
print({'exit_code':r.returncode,'controls':len(rows),'seconds':record['seconds'],'overload_differences':diffs})
