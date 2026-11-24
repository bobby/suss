"""Fresh UUID case dispatch controls use independently constructed selectors."""
from pathlib import Path
import hashlib,json,subprocess,time
OUT=Path(__file__).resolve().parent
cases=[]
def q(s):return json.dumps(s)
def add(name,expression):cases.append({'name':'uuid-case-'+name,'expression':expression})
u='#uuid "1-2-3-4-5"';canon='#uuid "00000001-0002-0003-0004-000000000005"'
for name,selector,constant in [
 ('fresh',(f'(read-string {q(u)})'),canon),
 ('vector',(f'(read-string {q("["+u+"]")})'),'['+canon+']'),
 ('list',(f'(read-string {q("("+u+")")})'),'('+canon+')'),
 ('map-value',(f'(read-string {q("{:id "+u+"}")})'),'{:id '+canon+'}'),
 ('map-key',(f'(read-string {q("{"+u+" :value}")})'),'{'+canon+' :value}'),
 ('set',(f'(read-string {q("#{"+u+"}")})'),'#{'+canon+'}'),
 ('nested',(f'(read-string {q("[{"+u+" #{["+u+"]}}]")})'),'[{'+canon+' #{['+canon+']}}]'),
]:
 add(name,f'(case {selector} {constant} :hit :miss)')
for i,s in enumerate(['00000000-0000-0000-0000-000000000000','ffffffff-ffff-ffff-ffff-ffffffffffff','80000000-0000-0001-ffff-fffffffffffe','7fffffff-ffff-fffe-8000-000000000001']):
 literal='#uuid '+q(s);add('signed-'+str(i),f'(case (read-string {q(literal)}) {literal} :hit :miss)')
zero='#uuid "00000000-0000-0000-0000-000000000000"'
add('zero-collision',f'(case (read-string {q(zero)}) nil :nil 0 :zero {zero} :uuid :miss)')
add('fresh-miss',f'(case (read-string {q(u)}) #uuid "2-3-4-5-6" :wrong :miss)')
add('single-evaluation',f'(let [n (atom 0) value (case (do (swap! n inc) (read-string {q(u)})) {u} :hit :miss)] [value @n])')
for label,selector,constant in [('fresh',f'(read-string {q(u)})',u),('zero',f'(read-string {q(zero)})',zero)]:
 add('live-equality-'+label,f'(let [trace (atom []) original = value (with-redefs [= (fn [a b] (swap! trace conj [(uuid? a) (uuid? b)]) (original a b))] (case {selector} {constant} :hit :miss))] [value @trace])')
add('live-hash-independent',f'(with-redefs [hash (fn [_] (throw (ex-info "public hash" {{}})))] (case (read-string {q(u)}) {u} :hit :miss))')
# Expansion must reject equal UUID constants even when their source spellings differ.
for expression in [f'(case {u} {u} :a {canon} :b :miss)',f'(case [{u}] [{u}] :a [{canon}] :b :miss)']:
 add('duplicate-'+str(len(cases)),f'(try (macroexpand (quote {expression})) :accepted (catch Throwable e :rejected))')
source=OUT/'uuid-case-oracle.clj';source.write_text('(assert (= "1.12.6" (clojure-version)))\n'+''.join('(println (pr-str (pr-str '+row['expression']+')))\n' for row in cases))
primary=json.loads((OUT/'reader-oracle-command.json').read_text());command=primary['command'][:-1]+[str(source)]
start=time.time();r=subprocess.run(command,stdout=subprocess.PIPE,stderr=subprocess.PIPE);(OUT/'uuid-case-oracle.stdout').write_bytes(r.stdout);(OUT/'uuid-case-oracle.stderr').write_bytes(r.stderr)
record={'command':command,'exit_code':r.returncode,'seconds':time.time()-start,'source_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'jars':primary['jars'],'stdout_sha256':hashlib.sha256(r.stdout).hexdigest()};(OUT/'uuid-case-oracle-command.json').write_text(json.dumps(record,indent=2)+'\n')
if r.returncode:print(r.stderr.decode()[-1600:]);raise SystemExit(r.returncode)
values=[json.loads(line) for line in r.stdout.decode().splitlines()];assert len(values)==len(cases)
for row,value in zip(cases,values):row['expected']=value
(OUT/'uuid-case-cases.json').write_text(json.dumps(cases,indent=2)+'\n');print('Recorded',len(cases),'fresh UUID case controls',flush=True)
