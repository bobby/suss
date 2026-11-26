"""Conserve actual binding-effects executions and the unchanged primary baseline."""
from pathlib import Path
import gzip,hashlib,json,re
ROOT=Path(__file__).resolve().parents[2];BASE=Path(__file__).resolve().parent
sha=lambda raw:hashlib.sha256(raw).hexdigest()
read=lambda p:gzip.decompress(p.read_bytes())
p=BASE/'binding-effects-focused/commands.json.gz';r=json.loads(read(p))
assert r['complete'] and r['inputs_unchanged'] and r['head_unchanged'] and r['binaryen_unchanged']
assert len(r['commands'])==3 and all(c['complete'] and c['exit_code']==0 for c in r['commands'])
assert sha((p.parent/'run.py').read_bytes())==r['runner_sha256']
prose={'results/m12-eval/README.md','docs/compatibility/README.md','docs/roadmap/handoff.md','docs/runtime/dynamic-bindings.md'}
comment_file='crates/suss-compile/tests/runtime_dynamic_switch.rs'
comment_before=b'// Pop restores the root through binding-set, which keeps the dynamic bit,'
comment_after=b'// Pop leaves the root untouched and keeps the dynamic bit,'
for name,digest in r['inputs'].items():
 if name in prose:continue
 raw=(ROOT/name).read_bytes()
 if name==comment_file:
  assert raw.count(comment_after)==1 and comment_before not in raw
  raw=raw.replace(comment_after,comment_before)
 assert sha(raw)==digest,'executed source/test/primary input changed: '+name

log=read(p.parent/'complete-eval-dynamic-and-async-targets.log.gz').decode()
assert '46 passed; 0 failed' in log and '62 passed; 0 failed' in log
assert 'test actual_macro_eval_preserves_binding_body_root_definitions ... ok' in log
assert 'test actual_macro_eval_preserves_binding_initializer_effects ... ok' in log
assert 'test suspended_binding_initializers_preserve_prior_outer_frame_effects ... ok' in log
assert 'test suspended_binding_body_keeps_root_definition_and_selected_frame_after_gc ... ok' in log
assert 'test fuel_interruption_during_frame_pop_preserves_initializer_root_effect ... ok' in log
assert 'test nested_multi_target_pop_is_restartable_at_every_fuel_boundary ... ok' in log
assert len(re.findall(r'test result: ok\. \d+ passed; 0 failed',log))==9
host=read(p.parent/'host-library-regressions.log.gz').decode();assert '118 passed; 0 failed' in host
ledger=re.findall(r'context-budget-ledger (Runtime|Macro) elements=(\d+) snapshot=(\d+) child=(\d+) public=(\d+) lower-bound=(\d+)',host)
assert len(ledger)==4 and all(int(public)>=int(bound)==int(snapshot)+int(child) for _,_,snapshot,child,public,bound in ledger)
for name,count,wrong in [('initializer-effects',5,3),('body-root-effects',6,5)]:
 folder=BASE/'binding-effects-before'/name;negative=json.loads((folder/'command.json').read_bytes());comparison=json.loads((folder/'comparison.json').read_bytes())
 assert negative['complete'] and negative['head_unchanged'] and negative['inputs_unchanged'] and negative['binary_unchanged']
 assert [c['exit_code'] for c in negative['commands']]==[0,0]
 assert comparison['exit_code']==1 and comparison['primary_exit_code']==comparison['native_exit_code']==0
 assert comparison['controls']==count and comparison['mismatch_count']==wrong
 assert (folder/'comparison.stdout').read_text().count('false')==wrong
first=json.loads(read(BASE/'binding-initializer-effects-focused/commands.json.gz'))
assert first['complete'] and first['inputs_unchanged'] and first['head_unchanged']
assert first['commands'][1]['exit_code']==101
assert sha((BASE/'binding-initializer-effects-focused/run.py').read_bytes())==first['runner_sha256']
interrupted=json.loads(read(BASE/'full-workspace-after-unbound-dynamic-fix/command.json.gz'))
assert interrupted['complete'] and interrupted['exit_code']==101 and interrupted['inputs_unchanged'] and interrupted['head_unchanged']
assert interrupted['command']==['cargo','test','--workspace','--locked','--','--test-threads=2']
assert sha((ROOT/'tests/oracle/clojure-test-suite-observations.json').read_bytes())=='449264ab6bca1dcf761d0204e4a64de4de91562e576615c4e84a63246839f4c7'
assert sha((ROOT/'tests/oracle/clojure-test-suite-known-failures.json').read_bytes())=='dea5db93e9548008d91e5c79ee11978920575975ec601f1852139ff063329d7f'
print('Binding effects: nine complete focused targets and118host pass;46eval/62ABI, both actualmacro+asyncGC controls, fuel sweeps/4budgetledgers retained. Before executions0 have actual wrongvalue comparators1; predecessorfocused101/full101 retained. Originalprimary/baseline unchanged. Freshbroader/full/review/CI separate.')
