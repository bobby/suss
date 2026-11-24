"""Verify every retained pinned eval primary expression, observation and jar."""
from pathlib import Path
import hashlib,json,zipfile
ROOT=Path(__file__).resolve().parents[2];BASE=Path(__file__).resolve().parent
sha=lambda raw:hashlib.sha256(raw).hexdigest()
for name,count in [('primary',24),('live-vars-primary',10),('do-boundary-primary',11),('namespace-primary',15),('review-corrections-primary',14),('var-implementation-primary',2),('macro-declaration-primary',22),('alias-primary',15),('macro-environment-primary',3),('macro-failure-primary',5)]:
 p=BASE/name;command=json.loads((p/'command.json').read_text());rows=json.loads((p/'cases.json').read_text())
 assert command['exit_code']==0 and command['controls']==count==len(rows)
 if 'legacy_corpus_sha256' in command:
  assert sha((ROOT/'tests/oracle/symbol-conversion-cases.json').read_bytes())==command['legacy_corpus_sha256']
  assert [row['name'] for row in rows]==['symbol-var-thunk-meta','symbol-var-macro']
 assert all(sha(Path(path).read_bytes())==digest for path,digest in command['jars'].items())
 source=(p/'oracle.clj').read_bytes();stdout=(p/'stdout.log').read_bytes()
 assert sha(source)==command['source_sha256'] and sha(stdout)==command['stdout_sha256']
 assert [row['expected'] for row in rows]==[json.loads(line) for line in stdout.decode().splitlines()]
 actual=[line for line in source.decode().splitlines() if line.startswith('(println (pr-str (pr-str ')]
 assert actual==['(println (pr-str (pr-str '+row['expression']+')))' for row in rows]
 if (p/'macros.json').exists():
  assert all(definition in source.decode() for definition in json.loads((p/'macros.json').read_text()))
 if name=='alias-primary':
  upstream=(p/'upstream-alias.clj').read_bytes()
  assert sha(upstream)==command['upstream_alias_sha256']
  record=json.loads((BASE/'alias-source-provenance.json').read_text())
  assert sha(upstream)==record['upstream_alias_sha256']
  adapted=(ROOT/'docs/compatibility/bootstrap/exact-integers.sus').read_bytes().split(b';; BEGIN Clojure alias\n')[1].split(b';; END Clojure alias\n')[0]
  assert sha(adapted)==record['adapted_sha256']
  assert adapted in (ROOT/'runtime/core-import/suss/core.sus').read_bytes()
  jar=Path(next(path for path in command['jars'] if path.endswith('/clojure-1.12.6.jar')))
  with zipfile.ZipFile(jar) as archive: assert upstream in archive.read('clojure/core.clj')
 print(name,count,'primary expressions/observations/jars verified')
record=json.loads((BASE/'eval-source-provenance.json').read_text())
p=json.loads((BASE/'primary/command.json').read_text())
jar=Path(next(path for path in p['jars'] if path.endswith('/clojure-1.12.6.jar')))
with zipfile.ZipFile(jar) as archive:core=archive.read('clojure/core.clj')
assert sha(core)==p['entry_sha256']
text=(ROOT/'docs/compatibility/bootstrap/exact-integers.sus').read_bytes()
adapted=text.split(b';; BEGIN Clojure eval\n',1)[1].split(b';; END Clojure eval\n',1)[0]
# The recorded adaptation hashes the complete def form plus its final LF.
adapted=adapted[adapted.index(b'(def '):]
assert sha(adapted)==record['adapted_eval_sha256']
assert record['upstream_eval_sha256']==p['upstream_eval_sha256']
upstream=(BASE/'primary/upstream-eval.clj').read_bytes()
assert sha(upstream)==p['upstream_eval_sha256'] and upstream.rstrip() in core
print('Public eval adaptation hash verified; native execution has separate receipts')
