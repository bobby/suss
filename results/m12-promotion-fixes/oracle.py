from pathlib import Path
import hashlib,json,subprocess
OUT=Path(__file__).resolve().parent
jars=[Path('/home/bobby/.m2/repository')/p for p in ['org/clojure/clojure/1.12.6/clojure-1.12.6.jar','org/clojure/spec.alpha/0.5.238/spec.alpha-0.5.238.jar','org/clojure/core.specs.alpha/0.4.74/core.specs.alpha-0.4.74.jar']]
cases=[]
for value in ['1/3','-2/7','123456789012345678901234567890/991']:
 cases += [f'(unchecked-negate {value})',f'((fn [x] (unchecked-negate x)) {value})',f'((identity unchecked-negate) {value})',f'(let [x (unchecked-negate {value})] [(pr-str (class x)) (ratio? x) (= x (- {value}))])']
cases += ['(unchecked-inc 1/3)','(unchecked-dec 1/3)','(unchecked-add 1/3 2/3)','(unchecked-subtract 1/3 4/3)','(unchecked-multiply 2/3 3/4)','(unchecked-negate -9223372036854775808)','((fn [^long x] (unchecked-negate x)) -9223372036854775808)','(unchecked-negate 1N)','(unchecked-negate -18446744073709551616N)','(unchecked-negate 0.0)','(unchecked-negate -0.0)']
for expr in ['(biginteger 1)','(biginteger -18446744073709551616N)']:
 for op in ['unchecked-negate','-','abs','inc','dec']:
  cases.append(f'(let [x ({op} {expr})] [(pr-str x) (pr-str (class x))])')
command=['java','-cp',':'.join(map(str,jars)),'clojure.main','-e','\n'.join('(prn '+s+')' for s in cases)]
r=subprocess.run(command,text=True,capture_output=True)
(OUT/'oracle.json').write_text(json.dumps({'command':command,'jars':{str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in jars},'exit_code':r.returncode,'stdout':r.stdout,'stderr':r.stderr},indent=2)+'\n')
assert r.returncode==0,r.stderr
values=r.stdout.splitlines();assert len(values)==len(cases)
(OUT/'cases.json').write_text(json.dumps(list(zip(cases,values)),indent=2)+'\n')
print('fresh JVM unchecked/category cases',len(cases))
