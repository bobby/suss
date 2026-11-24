"""Record actual pinned Clojure behavior before porting numeric suite dependencies."""
from pathlib import Path
import hashlib,json,subprocess,time
OUT=Path(__file__).resolve().parent
cases=[]
def add(name,expression):cases.append({'name':name,'expression':expression})
# Exhaust all return positions across each specialized predicate arity and both
# sides of the three-argument/variadic boundary. Trace order is part of the value.
for predicates in range(1,6):
 for argc in range(0,7):
  positions=[None]+[(p,x) for p in range(predicates) for x in range(argc)]
  for hit in positions:
   pred=[]
   for p in range(predicates):
    result='nil' if p%2 else 'false'
    if hit is not None and p==hit[0]:result=f'(if (= x {hit[1]}) [:hit {p} x] {result})'
    pred.append(f'(fn [x] (swap! trace conj [{p} x]) {result})')
   args=' '.join(map(str,range(argc)))
   add(f'some-fn-{predicates}-pred-{argc}-arg-hit-{hit}',
       f'(let [trace (atom []) f (some-fn {" ".join(pred)}) value (f {args})] [value @trace])')
for value in ['0','""','[]','{}',':ok','false','nil']:
 add('some-fn-truthiness-'+value,f'(let [trace (atom []) f (some-fn (fn [x] (swap! trace conj x) {value}))] [(f 1 2 3 4) @trace])')
for source in ['(some-fn)','((some-fn "bad"))','((some-fn "bad") nil)','((some-fn 42) nil)','((some-fn #{1 2} #{3 4}) 5 3 7)','(fn? (some-fn :x))','((some-fn :x) {:x 0})']:
 add('some-fn-shape-'+str(len(cases)),f'(try {source} (catch Throwable e :throws))')
# Preserve key function effects, returned operand category and last-wins ties.
for args in ['[1]','[1 2]','[2 1 3]','[1N 2N]','[2N 1N 3N]','[1/2 1N 1.0]','[1.25M 1.2500M 2M]','[9007199254740993N 9007199254740992N 9007199254740994N]',
             '[##NaN 1]','[1 ##NaN]','[##-Inf 1 ##NaN]','[##-Inf ##NaN 1]','[1 ##-Inf ##NaN]','[1 ##NaN ##-Inf]','[##NaN ##-Inf 1]','[##NaN 1 ##-Inf]',
             '[0.0 -0.0]','[-0.0 0.0]','[1.0 1N 1M 1]']:
 add('min-key-values-'+str(len(cases)),f'(let [trace (atom []) value (apply min-key (fn [x] (swap! trace conj x) x) {args})] [(pr-str value) (mapv pr-str @trace)])')
for weights in ['[1]','[1 2]','[2 1 3]','[1 1 1 1]','[2 1 1 3 1]','[3 2 1 0 -1]']:
 add('min-key-key-effects-'+str(len(cases)),f'(let [trace (atom []) xs (map-indexed (fn [i k] {{:id i :k k}}) {weights}) value (apply min-key (fn [x] (swap! trace conj (:id x)) (:k x)) xs)] [(:id value) @trace])')
for args in ['[]','[identity]','[nil 1]','[nil 1 2]','[identity "x" "y"]','[identity [] []]']:
 add('min-key-shape-'+str(len(cases)),f'(try (apply min-key {args}) (catch Throwable e :throws))')
add('min-key-key-exception-order','(let [trace (atom []) result (try (min-key (fn [x] (swap! trace conj x) (if (= x 2) (throw (ex-info "stop" {})) x)) 1 2 3 4) (catch Throwable e :throws))] [result @trace])')
assert len({c['name'] for c in cases})==len(cases)
(OUT/'inputs.json').write_text(json.dumps(cases,indent=2)+'\n')
source=OUT/'oracle.clj';source.write_text('(assert (= "1.12.6" (clojure-version)))\n'+''.join('(println (pr-str (pr-str '+c['expression']+')))\n' for c in cases))
jars=[Path.home()/('.m2/repository/org/clojure/'+p) for p in ['clojure/1.12.6/clojure-1.12.6.jar','spec.alpha/0.5.238/spec.alpha-0.5.238.jar','core.specs.alpha/0.4.74/core.specs.alpha-0.4.74.jar']]
command=['java','-Xmx512m','-cp',':'.join(map(str,jars)),'clojure.main',str(source)]
started=time.time();r=subprocess.run(command,stdout=subprocess.PIPE,stderr=subprocess.PIPE);(OUT/'oracle.stdout').write_bytes(r.stdout);(OUT/'oracle.stderr').write_bytes(r.stderr)
record={'command':command,'exit_code':r.returncode,'seconds':time.time()-started,'source_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'inputs_sha256':hashlib.sha256((OUT/'inputs.json').read_bytes()).hexdigest(),'jars':{str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in jars},'stdout_sha256':hashlib.sha256(r.stdout).hexdigest()}
(OUT/'oracle-command.json').write_text(json.dumps(record,indent=2)+'\n')
if r.returncode:print(r.stderr.decode()[-1500:]);raise SystemExit(r.returncode)
values=[json.loads(line) for line in r.stdout.decode().splitlines()];assert len(values)==len(cases)
for case,value in zip(cases,values):case['expected']=value
(OUT/'cases.json').write_text(json.dumps(cases,indent=2)+'\n');print('Fresh JVM controls',len(cases),flush=True)
