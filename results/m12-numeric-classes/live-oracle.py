from pathlib import Path
import hashlib, json, subprocess

OUT = Path(__file__).resolve().parent
jars = [Path('/home/bobby/.m2/repository') / p for p in [
    'org/clojure/clojure/1.12.6/clojure-1.12.6.jar',
    'org/clojure/spec.alpha/0.5.238/spec.alpha-0.5.238.jar',
    'org/clojure/core.specs.alpha/0.4.74/core.specs.alpha-0.4.74.jar']]
observe = '(mapv live-call [1 1.0 1N (biginteger 1) 1/2])'
steps = [
    {'setup': '(defprotocol LiveNumeric (live-value [x])) (extend-type Number LiveNumeric (live-value [x] :base)) (def live-call (fn [x] (live-value x)))', 'source': observe},
    {'setup': '(extend-type Long LiveNumeric (live-value [x] :long))', 'source': observe},
    {'setup': '(extend-type Number LiveNumeric (live-value [x] :changed))', 'source': observe},
    {'setup': '(extend-type clojure.lang.BigInt LiveNumeric (live-value [x] :bigint))', 'source': observe},
    {'setup': '(extend-type Class LiveNumeric (live-value [x] :class))', 'source': '(mapv live-call [Long Double clojure.lang.BigInt java.math.BigInteger clojure.lang.Ratio Class])'},
    {'setup': '', 'source': '[(apply live-value [1N]) (apply live-value [1.0]) (live-call 1) (live-call 1N) (live-call 1.0)]'},
    {'setup': '(defprotocol NumericMarker) (extend-type Number NumericMarker) (def marker-call (fn [x] (satisfies? NumericMarker x)))', 'source': '(mapv marker-call [1 1.0 1N (biginteger 1) 1/2 nil Long])'},
    {'setup': '(extend-type Class NumericMarker)', 'source': '(mapv marker-call [1 1N Long Class nil])'},
]
program = '\n'.join(s['setup'] + '\n(prn ' + s['source'] + ')' for s in steps)
command = ['java', '-cp', ':'.join(map(str, jars)), 'clojure.main', '-e', program]
p = subprocess.run(command, text=True, capture_output=True)
(OUT / 'live-oracle-attempt.json').write_text(json.dumps({'command':command,'stdout':p.stdout,'stderr':p.stderr,'exit_code':p.returncode},indent=2)+'\n')
assert p.returncode == 0, p.stderr
# defprotocol prints its name; retain those lines in the raw receipt, but the
# observations deliberately all print vectors, so selection is unambiguous.
values = [line for line in p.stdout.splitlines() if line.startswith('[')]
assert len(values) == len(steps), p.stdout
for step, expected in zip(steps, values):
    step['expected'] = expected
(OUT / 'live-cases.json').write_text(json.dumps(steps, indent=2) + '\n')
(OUT / 'live-oracle.json').write_text(json.dumps({'command': command, 'jars': {str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in jars}, 'source': program, 'exit_code': p.returncode, 'stdout': p.stdout, 'stderr': p.stderr}, indent=2) + '\n')
print(values)
