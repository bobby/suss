"""Fresh primary expectations for the raw compiled-fragment numeric decoder."""
from pathlib import Path
import hashlib, json, subprocess, time
ROOT = Path(__file__).resolve().parents[2]
OUT = Path(__file__).resolve().parent
expressions = [
    '(/ 4)', '(/ 8 12)', '(/ -8 12)', '(/ 2 1)',
    '(/ 9223372036854775807 2)', '(/ 18446744073709551617N 3)',
    '(let [x 4] (/ x))', '(/ 4.0)', '(/ 20 2 5)',
    '0N', '-18446744073709551617N',
    '(/ 18446744073709551617N 18446744073709551617N)',
]
source = '(assert (= "1.12.6" (clojure-version)))\n'
for expr in expressions:
    source += ('(let [v ' + expr + '] (println (pr-str (.getName (class v)))) '
               '(println (pr-str (if (instance? Double v) '
               '(Long/toHexString (Double/doubleToRawLongBits v)) (pr-str v)))))\n')
(OUT / 'oracle.clj').write_text(source)
prior = json.loads((ROOT / 'results/m12-wide-wit/oracle-command.json').read_text())
command = prior['command'][:-1] + [str(OUT / 'oracle.clj')]
started = time.time()
r = subprocess.run(command, cwd=ROOT, text=True, capture_output=True)
(OUT / 'oracle.log').write_text(r.stdout + r.stderr)
(OUT / 'oracle-command.json').write_text(json.dumps({
    'command': command, 'exit_code': r.returncode, 'seconds': time.time() - started,
    'source_sha256': hashlib.sha256(source.encode()).hexdigest(),
    'jars': {p: hashlib.sha256(Path(p).read_bytes()).hexdigest() for p in prior['jars']},
}, indent=2) + '\n')
assert r.returncode == 0, r.stderr
lines = [json.loads(line) for line in r.stdout.splitlines()]
assert len(lines) == len(expressions) * 2
cases = []
for i, expression in enumerate(expressions):
    cls, text = lines[i * 2:i * 2 + 2]
    if cls == 'clojure.lang.Ratio':
        n, d = text.split('/')
        expected = {'tag': 'ratio', 'numerator': n, 'denominator': d}
    elif cls in ['clojure.lang.BigInt', 'java.math.BigInteger']:
        expected = {'tag': 'bigint', 'value': text.removesuffix('N')}
    elif cls == 'java.lang.Long':
        expected = {'tag': 'long', 'value': text}
    elif cls == 'java.lang.Double':
        expected = {'tag': 'f64', 'bits': text.zfill(16)}
    else:
        raise AssertionError((expression, cls, text))
    cases.append({'source': expression, 'class': cls, 'expected': expected})
(OUT / 'cases.json').write_text(json.dumps(cases, indent=2) + '\n')
print(len(cases), 'fresh Clojure 1.12.6 numeric observations')
