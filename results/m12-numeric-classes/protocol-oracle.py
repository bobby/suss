from pathlib import Path
import subprocess,json
out=Path(__file__).resolve().parent
source='''(defprotocol NumericClassProtocol (numeric-class-kind [x]))
(extend-type java.lang.Long NumericClassProtocol (numeric-class-kind [x] :long))
(extend-type java.lang.Double NumericClassProtocol (numeric-class-kind [x] :double))
(extend-type clojure.lang.BigInt NumericClassProtocol (numeric-class-kind [x] :bigint))
(extend-type java.math.BigInteger NumericClassProtocol (numeric-class-kind [x] :biginteger))
(extend-type clojure.lang.Ratio NumericClassProtocol (numeric-class-kind [x] :ratio))
'''
cases=['(mapv numeric-class-kind [1 1.0 1N (biginteger 1) 1/2 2 2.0 2N (biginteger 2) 2/3])','[(satisfies? NumericClassProtocol 1) (satisfies? NumericClassProtocol 1.0) (satisfies? NumericClassProtocol 1N) (satisfies? NumericClassProtocol (biginteger 1)) (satisfies? NumericClassProtocol 1/3) (satisfies? NumericClassProtocol nil)]','(let [f (fn [x] (numeric-class-kind x))] [(f 1) (f 1.0) (f 1N) (f (biginteger 1)) (f 1/3) (f 2) (f 2.0)])']
cp='/home/bobby/.m2/repository/org/clojure/clojure/1.12.6/clojure-1.12.6.jar:/home/bobby/.m2/repository/org/clojure/spec.alpha/0.5.238/spec.alpha-0.5.238.jar:/home/bobby/.m2/repository/org/clojure/core.specs.alpha/0.4.74/core.specs.alpha-0.4.74.jar'
program=source+'\n'+'\n'.join('(prn '+s+')' for s in cases)
p=subprocess.run(['java','-cp',cp,'clojure.main','-e',program],text=True,capture_output=True)
assert p.returncode==0,p.stderr
outputs=p.stdout.splitlines()[-len(cases):]
(out/'protocol-definitions.sus').write_text(source)
(out/'protocol-cases.json').write_text(json.dumps([{'source':s,'expected':e} for s,e in zip(cases,outputs)],indent=2)+'\n')
(out/'protocol-oracle.json').write_text(json.dumps({'source':program,'exit_code':p.returncode,'stdout':p.stdout,'stderr':p.stderr},indent=2)+'\n');print(outputs)
