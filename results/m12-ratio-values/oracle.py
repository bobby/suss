"""Record exact fraction expectations from the pinned primary JVM oracle."""
from pathlib import Path
import hashlib
import json
import random
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]
OUT = Path(__file__).resolve().parent
initial = json.loads((ROOT / 'results/m12-ratio-oracle/cases.json').read_text())
forms = [source for source, _ in initial]
expressions = [
    '[(/ 1 2) (/ -1 2) (/ 1 -2) (/ -1 -2) (/ 0 3) (/ 0N 3) (/ 2 2) (/ 2N 2)]',
    '[(/ -9223372036854775808 -1) (/ 1 -9223372036854775808) (/ -9223372036854775808 3)]',
    '[(+ 1/3 1) (+ 1 1/3) (- 1/3 1) (- 1 1/3) (* 1/3 0) (* 1/3 0N)]',
    '[(+ 1/3 0.5) (- 0.5 1/3) (* 1/3 0.5) (/ 0.5 1/3) (quot 1/3 0.25) (rem 1/3 0.25)]',
    '[(double 1/7) (double -1/7) (double 2/7) (double 1/9999999999999999999999999999999999999999)]',
    '[(double 99999999999999995/10000000000000000) (double 99999999999999985/10000000000000000)]',
    '[(< 1/7 0.14285714285714285) (== 1/7 0.14285714285714285) (compare 1/7 0.14285714285714285)]',
    '[(= 1/2 0.5) (= 1/2 2/4) (= 1/3 2/6) (== 1/2 0.5) (== 1/3 1/3)]',
    '[(compare -2/3 -1/2) (compare -1/2 -2/3) (compare 2/3 4/6) (< -2/3 -1/2) (<= 2/3 4/6)]',
    '[(bigint 18446744073709551617/2) (biginteger -18446744073709551617/2) (long 18446744073709551617/2)]',
    '[(int 4294967297/2) (unchecked-int 4294967297/2) (unchecked-long 18446744073709551617/2)]',
    '[(rationalize 0.0) (rationalize -0.0) (rationalize 1.0) (rationalize 1e-20) (rationalize 1.0000000000000002e20)]',
    '[(integer? 1/3) (rational? nil) (ratio? nil) (number? 1/3) (zero? 1/3) (pos? 1/3) (neg? -1/3)]',
    '[(abs -1/3) (abs 1/3) (min 1/3 0.5) (max 1/3 0.5)]',
    '(let [x 1/3] [(str x) (pr-str x) (str (numerator x)) (str (denominator x))])',
    '[(read-string "2/6") (suss.edn/read-string "-2/6") (read-string "0/9223372036854775808")]',
    '(let [m (into {1/3 :ratio} (map (fn [x] [x x]) (range 40)))] [(get m 2/6) (contains? m 1/3) (count m)])',
    '(let [x 1/3] [(= x x) (identical? x x) (hash x)])',
    '(let [x (atom [])] [(+ (do (swap! x conj 1) 1/3) (do (swap! x conj 2) 2/3)) @x])',
    '(bit-and 1/3 1)', '(even? 1/3)', '(/ 1/3 0N)', '(quot 1/3 0)', '(rem 1/3 0)',
]
expressions += [
    '(long 18446744073709551617/2)', '(long -18446744073709551617/2)',
    '(unchecked-long 18446744073709551617/2)', '(unchecked-long -18446744073709551617/2)',
    '(int 4294967297/2)', '(unchecked-int 4294967297/2)',
    '(unchecked-int -4294967297/2)', '(unchecked-int 8589934595/2)',
    '(byte 257/2)', '(short 65537/2)', '(unchecked-byte 257/2)', '(unchecked-short 65537/2)',
]
rng = random.Random(3380602)
for limbs in [1, 2, 4, 8, 16, 32]:
    n = rng.getrandbits(32 * limbs) + 1
    d = rng.getrandbits(32 * limbs) + 2
    for sign in [1, -1]:
        a, b = f'{sign*n}/{d}', f'{d}/{n+1}'
        expressions.append(f'[(+ {a} {b}) (- {a} {b}) (* {a} {b}) (/ {a} {b}) (quot {a} {b}) (rem {a} {b}) (hash {a}) (compare {a} {b})]')
# Long-scale and overflow/underflow observations also distinguish the decimal64
# conversion from dividing independently rounded numerator/denominator doubles.
for exponent in [300, 324, 400]:
    power = 10**exponent
    expressions.append(f'[(double 1/{power+1}) (double -1/{power+1}) (double {power+1}/3)]')
forms += [f'(pr-str (try {form} (catch :default e :error)))' for form in expressions]
forms += [source for source, _ in json.loads((OUT / 'edge-cases.json').read_text())]
source = '(assert (= "1.12.6" (clojure-version)))\n' + '\n'.join(
    '(println (pr-str ' + form.replace('(catch :default e', '(catch Throwable e').replace('suss.edn/read-string', 'clojure.edn/read-string').replace('(.-message e)', '(.getMessage e)') + '))' for form in forms) + '\n'
source = "(require 'clojure.edn)\n" + source
(OUT / 'oracle.clj').write_text(source)
prior = json.loads((ROOT / 'results/m12-ratio-oracle/oracle-command.json').read_text())
command = prior['command'][:-1] + [str(OUT / 'oracle.clj')]
started = time.time()
result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True)
(OUT / 'oracle.log').write_text(result.stdout + result.stderr)
receipt = {'command': command, 'exit_code': result.returncode, 'seconds': time.time()-started,
    'source_sha256': hashlib.sha256(source.encode()).hexdigest(),
    'jars': {path: hashlib.sha256(Path(path).read_bytes()).hexdigest() for path in prior['jars']}}
(OUT / 'oracle-command.json').write_text(json.dumps(receipt, indent=2) + '\n')
assert result.returncode == 0, result.stderr
expected = [json.loads(line) for line in result.stdout.splitlines()]
assert len(expected) == len(forms)
(OUT / 'cases.json').write_text(json.dumps(list(zip(forms, expected)), indent=2) + '\n')
print(len(forms), 'fresh primary Ratio observations')
