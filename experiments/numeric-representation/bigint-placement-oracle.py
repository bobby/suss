"""Fresh JVM Clojure arithmetic for the probe's complete operand corpus."""
from pathlib import Path
import json,subprocess,time,hashlib
root=Path(__file__).resolve().parents[2];out=root/'results/m12-bigint-placement';cases=json.loads((out/'cases.json').read_text())
home=Path.home();jars=[home/'.m2/repository'/p for p in ['org/clojure/clojure/1.12.6/clojure-1.12.6.jar','org/clojure/spec.alpha/0.5.238/spec.alpha-0.5.238.jar','org/clojure/core.specs.alpha/0.4.74/core.specs.alpha-0.4.74.jar']]
rows=['(assert (= "1.12.6" (clojure-version)))']
for i,c in enumerate(cases):
 number=lambda xs:sum(x<<(32*j) for j,x in enumerate(xs))
 a,b=format(number(c['a']),'x'),format(number(c['b']),'x')
 rows.append('(let [a (bigint (java.math.BigInteger. '+json.dumps(a)+' 16)) b (bigint (java.math.BigInteger. '+json.dumps(b)+' 16)) actual [(.toString (biginteger (+\' a b)) 16) (.toString (biginteger (*\' a b)) 16)]] (prn '+str(i)+' actual) (assert (= ['+json.dumps(c['add_hex'])+' '+json.dumps(c['mul_hex'])+'] actual)))')
(out/'oracle.clj').write_text('\n'.join(rows)+'\n');cmd=['java','-cp',':'.join(map(str,jars)),'clojure.main',str(out/'oracle.clj')];start=time.time()
with (out/'oracle.log').open('w') as log:r=subprocess.run(cmd,stdout=log,stderr=subprocess.STDOUT)
(out/'oracle-command.json').write_text(json.dumps({'command':cmd,'exit_code':r.returncode,'seconds':time.time()-start,'source_sha256':hashlib.sha256((out/'oracle.clj').read_bytes()).hexdigest(),'jars':{str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in jars}},indent=2)+'\n')
assert r.returncode==0
print(f'{len(cases)*2} exact JVM arithmetic results match',flush=True)
