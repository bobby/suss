from pathlib import Path
import gzip,hashlib,json
root=Path(__file__).resolve().parents[2];sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
p=root/'results/m12-eval/lazy-math-context-frame-focused/commands.json.gz';r=json.loads(gzip.decompress(p.read_bytes()))
assert r['complete'] and r['inputs_unchanged'] and r['head_unchanged'] and r['binaryen_unchanged']
assert len(r['commands'])==3 and all(x['complete'] and x['exit_code']==0 for x in r['commands'])
prose={'results/m12-eval/README.md','docs/compatibility/README.md','docs/roadmap/handoff.md'}
protected={p:h for p,h in r['inputs'].items() if p not in prose}
assert all((root/p).is_file() and sha(root/p)==h for p,h in protected.items()), 'Validated source/test/primary/config input changed'
base=root/'results/m12-eval/lazy-math-context-primary';d=json.loads((base/'command.json').read_text());rows=json.loads((base/'cases.json').read_text())
assert d['exit_code']==0 and len(rows)==20
assert sha(base/'expression.clj')==d['source_sha256'] and sha(base/'primary.stdout')==d['stdout_sha256']
assert all(sha(Path(p))==h for p,h in d['jars'].items())
assert (base/'primary.stdout').read_text().splitlines()==[r['observed_edn'] for r in rows]
clj='(assert (= "1.12.6" (clojure-version)))\n'+'\n'.join('(prn '+r['expression']+')' for r in rows)+'\n'
assert (base/'expression.clj').read_text()==clj
native='['+'\n'.join('(try '+r['expression'].replace('(catch Throwable ', '(catch :default ')+' (catch :default unexpected :unexpected-native-error))' for r in rows)+']\n'
assert (base/'expression.sus').read_text()==native
print('Exact frozen focused-source conservation and all20 primary/adapted observations verified; this is not a broader/workspace or issue-closure claim.')
