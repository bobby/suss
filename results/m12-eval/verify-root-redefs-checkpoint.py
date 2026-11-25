"""Conserve executed root redefinition and thread-binding semantics/evidence."""
from pathlib import Path
import gzip,hashlib,json,re,subprocess,tarfile
ROOT=Path(__file__).resolve().parents[2];BASE=Path(__file__).resolve().parent
sha=lambda raw:hashlib.sha256(raw).hexdigest();read=lambda p:gzip.decompress(p.read_bytes())
folder=BASE/'with-redefs-root-focused-after-ir-kind';r=json.loads(read(folder/'commands.json.gz'))
assert r['complete'] and r['inputs_unchanged'] and r['head_unchanged'] and r['binaryen_unchanged']
assert len(r['commands'])==3 and all(c['complete'] and c['exit_code']==0 for c in r['commands'])
assert sha((folder/'run.py').read_bytes())==r['runner_sha256']
prose={'results/m12-eval/README.md','docs/compatibility/README.md','docs/roadmap/handoff.md','docs/runtime/dynamic-bindings.md'}
assert all((ROOT/p).is_file() and sha((ROOT/p).read_bytes())==h for p,h in r['inputs'].items() if p not in prose),'executed source/test/primary input changed'
log=read(folder/'complete-eval-dynamic-and-async-targets.log.gz').decode()
counts=[int(x) for x in re.findall(r'test result: ok\. (\d+) passed; 0 failed',log)]
assert len(counts)==11 and sum(counts)==200 and 48 in counts and 62 in counts
for name in ['root_redefinitions_restore_roots_and_respect_existing_thread_bindings','actual_macro_eval_distinguishes_root_redefinitions_from_thread_bindings','root_redefinition_install_and_pop_are_recoverable_at_every_fuel_boundary','root_leases_restore_roots_without_overriding_bindings_or_forking_ownership','pending_root_redefinitions_are_global_and_restore_on_return_or_cancellation_after_gc']:
 assert 'test '+name+' ... ok' in log
host=read(folder/'host-library-regressions.log.gz').decode();assert '118 passed; 0 failed' in host
ledger=re.findall(r'context-budget-ledger (Runtime|Macro) elements=(\d+) snapshot=(\d+) child=(\d+) public=(\d+) lower-bound=(\d+)',host)
assert len(ledger)==4 and all(int(public)>=int(bound)==int(snapshot)+int(child) for _,_,snapshot,child,public,bound in ledger)
folder=BASE/'with-redefs-root-before';before=json.loads((folder/'command.json').read_bytes());comparison=json.loads((folder/'comparison.json').read_bytes())
assert before['complete'] and before['head_unchanged'] and before['inputs_unchanged'] and before['binary_unchanged']
assert before['head']=='50d70b62522e3d442d479fa71bac23d91a7a106e'
assert [c['exit_code'] for c in before['commands']]==[0,0]
assert comparison['exit_code']==1 and comparison['mismatch_count']==4 and comparison['controls']==6
assert comparison['primary_exit_code']==comparison['native_exit_code']==0
assert (folder/'comparison.stdout').read_text().count('false')==4
rows=json.loads((BASE/'with-redefs-root-primary/cases.json').read_bytes())
old_rows=json.loads((folder/'cases.json').read_bytes())
assert [r['expression'] for r in rows[:6]]==[r['expression'] for r in old_rows]
assert (folder/'primary.stdout').read_text().strip()=='['+' '.join((BASE/'with-redefs-root-primary/primary.stdout').read_text().splitlines()[:6])+']'

with tarfile.open(folder/'source-before.tar.gz') as t:
 for member in t.getmembers():
  raw=t.extractfile(member).read();assert raw==subprocess.check_output(['git','show',before['head']+':'+member.name],cwd=ROOT)
  if member.name in before['inputs']:assert sha(raw)==before['inputs'][member.name]
first=BASE/'with-redefs-root-focused';failed=json.loads(read(first/'commands.json.gz'))
assert failed['complete'] and failed['inputs_unchanged'] and failed['head_unchanged'] and failed['commands'][1]['exit_code']==101
assert sha((first/'run.py').read_bytes())==failed['runner_sha256']
old=read(first/'complete-eval-dynamic-and-async-targets.log.gz').decode()
assert 'FAILED. 46 passed; 2 failed' in old and 'IR dynamic snapshot must be Value' in old
with tarfile.open(first/'source-before.tar.gz') as t:
 for member in t.getmembers():assert sha(t.extractfile(member).read())==failed['inputs'][member.name]
w=json.loads(read(BASE/'full-workspace-after-binding-effects-fix/command.json.gz'))
assert w['complete'] and w['exit_code']==101 and w['head_unchanged'] and w['inputs_unchanged'] and w['head']==before['head']
b=json.loads(read(BASE/'binding-effects-broader/commands.json.gz'))
assert b['complete'] and b['head_unchanged'] and b['inputs_unchanged'] and b['binaryen_unchanged']
assert all(x['exit_code']==0 for x in b['commands'][:-1]) and b['commands'][-1]['exit_code']==101 and not b['required_workspace_passed']
assert sha((ROOT/'tests/oracle/clojure-test-suite-observations.json').read_bytes())=='449264ab6bca1dcf761d0204e4a64de4de91562e576615c4e84a63246839f4c7'
assert sha((ROOT/'tests/oracle/clojure-test-suite-known-failures.json').read_bytes())=='dea5db93e9548008d91e5c79ee11978920575975ec601f1852139ff063329d7f'
print('Root redefinitions conserved:200target+118host=318 tests0,12primary controls, actualmacros/asyncGC/forkownership/2000fuel sweep and4budgetledgers. Genuinebefore0/wrongvaluecomparator1, initialIR101 and previousfull101 remain separate. Freshbroader/full/review/CI still required.')
