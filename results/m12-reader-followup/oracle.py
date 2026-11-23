"""Fresh pinned JVM controls for the remaining numeric suite reader dependencies."""
from pathlib import Path
import hashlib,json,subprocess,time
OUT=Path(__file__).resolve().parent
cases=[]
def add(name,source,jvm=None):cases.append({'name':name,'expression':source,'jvm_expression':jvm or source})
def q(s):return json.dumps(s,ensure_ascii=True)
valid=['550e8400-e29b-41d4-a716-446655440000','00000000-0000-0000-0000-000000000000','ffffffff-ffff-ffff-ffff-ffffffffffff','80000000-0000-0000-0000-000000000000','7fffffff-ffff-ffff-ffff-ffffffffffff','1-1-1-1-1','1-2-3-4-5','550E8400-E29B-41D4-A716-446655440000','+1-+2-+3-+4-+5','000000001-2-3-4-5','1-00002-3-4-5','1-2-00003-4-5','1-2-3-00004-5','1-2-3-4-0000000000005','\uff11-\uff12-\uff13-\uff14-\uff15','\u0661-\u0662-\u0663-\u0664-\u0665']
for i,s in enumerate(valid):
 literal='#uuid '+q(s)
 for op in ['str','pr-str','hash','uuid?','number?','integer?','decimal?']:
  add(f'uuid-valid-{i}-{op}',f'({op} {literal})')
 add(f'uuid-source-vs-runtime-{i}',f'(= {literal} (read-string {q(literal)}))')
 add(f'uuid-edn-vs-source-{i}',f'(= {literal} (suss.edn/read-string {q(literal)}))',f'(= {literal} (clojure.edn/read-string {q(literal)}))')
invalid=['','not-a-uuid','1-2-3-4','1-2-3-4-5-6','-1-2-3-4-5','1--2-3-4-5','1-2-3-4-','1-2-3-4-g','0x1-2-3-4-5','12345678901234567-2-3-4-5','0000000000000000000000000001-2-3-4-5',' 1-2-3-4-5','1-2-3-4-5 ','1-2-3-4-\uff27']
for i,s in enumerate(invalid):
 literal='#uuid '+q(s)
 add(f'uuid-invalid-{i}',f'(try (suss.edn/read-string {q(literal)}) (catch :default e :error))',f'(try (clojure.edn/read-string {q(literal)}) (catch Throwable e :error))')
for i,payload in enumerate(['nil','0',':x','[1]','()']):
 literal='#uuid '+payload
 add(f'uuid-nonstring-{i}',f'(try (suss.edn/read-string {q(literal)}) (catch :default e :error))',f'(try (clojure.edn/read-string {q(literal)}) (catch Throwable e :error))')
u='#uuid "550e8400-e29b-41d4-a716-446655440000"'
add('uuid-equal-distinct',f'[ (= {u} {u}) (identical? {u} {u})]')
add('uuid-key-and-set',f'[(get {{{u} 42}} {u}) (count #{{{u}}})]')
add('uuid-case-value',f'(case {u} {u} :hit :miss)')
add('uuid-reader-override',f'(suss.edn/read-string {{:readers {{\'uuid (fn [x] [:override x])}}}} {q(u)})',f'(clojure.edn/read-string {{:readers {{\'uuid (fn [x] [:override x])}}}} {q(u)})')
add('uuid-reader-default',f'(= {u} (suss.edn/read-string {{:default (fn [tag x] :default)}} {q(u)}))',f'(= {u} (clojure.edn/read-string {{:default (fn [tag x] :default)}} {q(u)}))')
for left,right in [(valid[0],valid[1]),(valid[3],valid[4]),(valid[2],valid[3]),('00000000-0000-0000-8000-000000000000','00000000-0000-0000-7fff-ffffffffffff')]:
 add('uuid-signed-compare-'+str(len(cases)),f'(compare #uuid {q(left)} #uuid {q(right)})')
for i,pattern in enumerate([r'#"my regex"',r'#""',r'#"\d+"',r'#"a\"b"',r'#"(?i)a"',r'#"[a-z&&[^bc]]"',r'#"(?<=a)b"',r'#"(a)\1"']):
 add(f'regex-str-{i}',f'(str {pattern})')
 add(f'regex-pr-str-{i}',f'(pr-str {pattern})')
 add(f'regex-self-{i}',f'(let [r {pattern}] [(= r r) (identical? r r) (number? r)])')
 add(f'regex-distinct-{i}',f'(= {pattern} {pattern})')
(OUT/'reader-inputs.json').write_text(json.dumps(cases,indent=2)+'\n')
jar=Path.home()/'.m2/repository/org/clojure/clojure/1.12.6/clojure-1.12.6.jar';jars=[jar,Path.home()/'.m2/repository/org/clojure/spec.alpha/0.5.238/spec.alpha-0.5.238.jar',Path.home()/'.m2/repository/org/clojure/core.specs.alpha/0.4.74/core.specs.alpha-0.4.74.jar']
source=OUT/'reader-oracle.clj';source.write_text('(require \'clojure.edn)\n(assert (= "1.12.6" (clojure-version)))\n'+''.join('(println (pr-str (pr-str '+c['jvm_expression']+')))\n' for c in cases))
command=['java','-cp',':'.join(str(p) for p in jars),'clojure.main',str(source)];started=time.time();r=subprocess.run(command,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True);(OUT/'reader-oracle.log').write_text(r.stdout)
receipt={'command':command,'exit_code':r.returncode,'seconds':time.time()-started,'source_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'jars':{str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in jars},'stdout_sha256':hashlib.sha256(r.stdout.encode()).hexdigest()};(OUT/'reader-oracle-command.json').write_text(json.dumps(receipt,indent=2)+'\n')
if r.returncode:print(r.stdout[-1600:]);raise SystemExit(r.returncode)
rows=[json.loads(line) for line in r.stdout.splitlines()];assert len(rows)==len(cases)
for row,expected in zip(cases,rows):row['expected']=expected
(OUT/'reader-cases.json').write_text(json.dumps(cases,indent=2)+'\n');print('Recorded',len(cases),'fresh JVM controls')
