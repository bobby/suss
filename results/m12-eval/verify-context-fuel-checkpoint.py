from pathlib import Path
import gzip,hashlib,json,re
root=Path(__file__).resolve().parents[2];sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
base=root/'results/m12-eval/context-fuel-accounting-focused';r=json.loads(gzip.decompress((base/'commands.json.gz').read_bytes()))
assert r['complete'] and r['inputs_unchanged'] and r['head_unchanged'] and r['binaryen_unchanged']
assert len(r['commands'])==3 and all(x['complete'] and x['exit_code']==0 for x in r['commands'])
prose={'results/m12-eval/README.md','docs/compatibility/README.md','docs/roadmap/handoff.md'}
protected={p:h for p,h in r['inputs'].items() if p not in prose}
assert all((root/p).is_file() and sha(root/p)==h for p,h in protected.items()), 'source/test/primary/config changed after focused validation'
log=gzip.decompress((base/'host-library-regressions.log.gz').read_bytes()).decode()
rows=re.findall(r'context-budget-ledger (Runtime|Macro) elements=(\d+) snapshot=(\d+) child=(\d+) public=(\d+) lower-bound=(\d+)',log)
assert len(rows)==4 and all(int(public)>=int(bound)==int(snapshot)+int(child) for _,_,snapshot,child,public,bound in rows)
assert '40 passed; 0 failed' in gzip.decompress((base/'complete-public-eval.log.gz').read_bytes()).decode()
negative=json.loads(gzip.decompress((root/'results/m12-eval/context-fuel-ledger-before/command.json.gz').read_bytes()))
assert negative['complete'] and negative['exit_code']==101 and negative['inputs_unchanged'] and negative['head_unchanged']
p=root/'results/m12-eval/lazy-math-context-primary';d=json.loads((p/'command.json').read_text());values=json.loads((p/'cases.json').read_text())
assert d['exit_code']==0 and len(values)==20 and sha(p/'expression.clj')==d['source_sha256'] and sha(p/'primary.stdout')==d['stdout_sha256']
assert all(sha(Path(p))==h for p,h in d['jars'].items())
assert (p/'primary.stdout').read_text().splitlines()==[x['observed_edn'] for x in values]
print('Actual focused source conserved; four measured whole-budget cases pass and real before101 retained; all20 primary values unchanged. Full workspace/review/CI remain separate.')
