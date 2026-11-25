"""Conserve production validation across two explicit test-harness corrections."""
from pathlib import Path
import gzip,hashlib,json,runpy
ROOT=Path(__file__).resolve().parents[2];B=Path(__file__).resolve().parent;sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest();head='c9eecf80df791baba8fc51c19cc7d6e1ea136179'
runpy.run_path(str(B/'verify-portable-dynamic-reference.py'))
def read(path):return json.loads(gzip.decompress(path.read_bytes()))
def passed(r):
 assert r['complete'] and r['inputs_unchanged'] and r['head_unchanged'] and r['head']==head
 assert all(c['complete'] and c['exit_code']==0 for c in r['commands'])
prose={'results/m12-eval/README.md','docs/compatibility/README.md','docs/roadmap/handoff.md','docs/runtime/dynamic-bindings.md'}
tests={'crates/suss-compile/tests/portable_pipeline.rs','crates/suss-compile/tests/portable_modules.rs'}
focused=read(B/'with-redefs-root-focused-after-ir-kind/commands.json.gz');assert focused['complete'] and focused['inputs_unchanged'] and focused['head_unchanged'] and all(c['exit_code']==0 for c in focused['commands'])
portable=read(B/'portable-dynamic-reference-focused/commands.json.gz');module=read(B/'portable-module-root-trace-focused/commands.json.gz')
for r in [portable,module]:passed(r)
for r,target,count in [(portable,'portable_pipeline',22),(module,'portable_modules',11)]:
 command=r['commands'][1]['command'];assert command==['cargo','test','--workspace','--locked','--test',target,'--','--nocapture','--test-threads=2']
 log=gzip.decompress((B/('portable-dynamic-reference-focused' if target=='portable_pipeline' else 'portable-module-root-trace-focused')/(r['commands'][1]['name']+'.log.gz')).read_bytes()).decode();assert f'test result: ok. {count} passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;' in log
assert sha(ROOT/'crates/suss-compile/tests/portable_pipeline.rs')==portable['inputs']['crates/suss-compile/tests/portable_pipeline.rs']
assert sha(ROOT/'crates/suss-compile/tests/portable_modules.rs')==module['inputs']['crates/suss-compile/tests/portable_modules.rs']
assert sha(B/'verify-portable-dynamic-reference.py')==module['inputs']['results/m12-eval/verify-portable-dynamic-reference.py']
broader=read(B/'root-redefs-broader/commands.json.gz');assert broader['complete'] and broader['inputs_unchanged'] and broader['head_unchanged'] and broader['head']==head
assert len(broader['commands'])==11 and all(c['complete'] and c['exit_code']==0 for c in broader['commands'][:10]) and broader['commands'][10]['exit_code']==101
for p,h in broader['inputs'].items():
 if p not in prose|tests:assert (ROOT/p).is_file() and sha(ROOT/p)==h,p
workspace=read(B/'full-workspace-after-root-redefs/command.json.gz');assert workspace['complete'] and workspace['exit_code']==101 and workspace['inputs_unchanged'] and workspace['head_unchanged']
print('Conserved318productionfocused + complete22portable/11module passes, C9bootstrap/native/strict10passes. Two test-only deltas explicit. C9full101 remains failure; fresh final-head exact workspace, review and CI still required.')
