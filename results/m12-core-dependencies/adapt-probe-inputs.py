"""Materialize pure fixture maps without depending on unported map-indexed."""
from pathlib import Path
import hashlib,json,re,subprocess,time
BASE=Path(__file__).resolve().parent
original=json.loads((BASE/'cases.json').read_text());adapted=[];changes=[]
pattern=r'\(map-indexed \(fn \[i k\] \{:id i :k k\}\) (\[[^]]*\])\)'
for row in original:
 expression=row['expression']
 def literal(match):
  weights=list(map(int,match[1][1:-1].split()))
  return '['+' '.join('{:id '+str(i)+' :k '+str(k)+'}' for i,k in enumerate(weights))+']'
 replacement=re.sub(pattern,literal,expression)
 adapted.append(row|{'original_expression':expression,'expression':replacement})
 if replacement!=expression:changes.append({'name':row['name'],'original':expression,'adapted':replacement})
assert len(changes)==6
(BASE/'probe-cases.json').write_text(json.dumps(adapted,indent=2)+'\n')
source=BASE/'probe-equivalence.clj';source.write_text('(assert (= "1.12.6" (clojure-version)))\n'+''.join('(println (pr-str (pr-str '+row['expression']+')))\n' for row in adapted))
primary=json.loads((BASE/'oracle-command.json').read_text());command=primary['command'][:-1]+[str(source)]
start=time.time();r=subprocess.run(command,stdout=subprocess.PIPE,stderr=subprocess.PIPE);(BASE/'probe-equivalence.stdout').write_bytes(r.stdout);(BASE/'probe-equivalence.stderr').write_bytes(r.stderr)
values=[json.loads(line) for line in r.stdout.decode().splitlines()] if r.returncode==0 else []
record={'command':command,'exit_code':r.returncode,'seconds':time.time()-start,'original_primary_sha256':hashlib.sha256((BASE/'cases.json').read_bytes()).hexdigest(),'adapted_inputs_sha256':hashlib.sha256((BASE/'probe-cases.json').read_bytes()).hexdigest(),'source_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'changes':changes,'jars':primary['jars'],'rows':len(values),'values_identical_to_primary':values==[row['expected'] for row in original],'qualification':'Only pure fixture construction changes; functions under test, key-function effects, expectations, IDs and coverage remain intact.'}
(BASE/'probe-equivalence-command.json').write_text(json.dumps(record,indent=2)+'\n')
assert r.returncode==0 and record['values_identical_to_primary'],record
p=BASE/'run-prototype-adapted.py';s=p.read_text().replace("BASE/'prototype-adapted-expression'","BASE/'prototype-materialized-fixtures'").replace("BASE/'cases.json'","BASE/'probe-cases.json'")
(BASE/'run-prototype-materialized.py').write_text(s)
print('All',len(values),'actual JVM values unchanged; materialized fixture constructors',len(changes),flush=True)
