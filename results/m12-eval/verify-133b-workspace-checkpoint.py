"""Conserve the completed exact baseline across prose/evidence publication only."""
from pathlib import Path
import gzip,hashlib,json,subprocess
ROOT=Path(__file__).resolve().parents[2];B=Path(__file__).resolve().parent;sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest();recorded='133b7024762d5006d4c784416e66ecef5a73f6b8'
def read(path):return json.loads(gzip.decompress(path.read_bytes()))
w=read(B/'full-workspace-after-portable-reference-alignment/command.json.gz');assert w['complete'] and w['exit_code']==0 and w['inputs_unchanged'] and w['head_unchanged'] and w['head']==recorded
assert w['command']==['cargo','test','--workspace','--locked','--','--test-threads=2']
prose={'results/m12-eval/README.md','docs/roadmap/handoff.md','docs/compatibility/README.md'}
for p,h in w['inputs'].items():
 if p not in prose:assert (ROOT/p).is_file() and sha(ROOT/p)==h,p
assert sha(ROOT/'results/m12-random/run-workspace.py')==w['runner_sha256']
g=read(B/'portable-reference-workspace-gates/commands.json.gz');assert g['complete'] and g['head']==recorded and g['head_unchanged'] and g['binaryen_unchanged'] and g['required_workspace_passed'] and g['actual_workspace_exit']==g['exit_code']==0
assert len(g['commands'])==4 and all(c['complete'] and c['exit_code']==0 for c in g['commands'])
assert sha(B/'portable-reference-workspace-gates/run.py')==g['runner_sha256']
allowed_prefixes=('results/m12-eval/full-workspace-after-portable-reference-alignment/','results/m12-eval/portable-reference-workspace-gates/','results/m12-eval/final-ci-133b/')
allowed_single={'results/m12-eval/verify-133b-workspace-checkpoint.py'}|prose
subprocess.run(['git','merge-base','--is-ancestor',recorded,'HEAD'],cwd=ROOT,check=True)
delta=subprocess.check_output(['git','diff','--name-only',recorded],cwd=ROOT,text=True).splitlines();assert all(p in allowed_single or p.startswith(allowed_prefixes) for p in delta),delta
ci=json.loads((B/'final-ci-133b/success.json').read_bytes());assert ci['head']==recorded and ci['actual_complete'] and ci['all_required_success'] and ci['actual_conclusion']=='success'
expected={'foundation','test',*[f'integration ({i})' for i in range(8)]};assert {j['name'] for j in ci['required_jobs']}==expected and all(j['status']=='completed' and j['conclusion']=='success' for j in ci['required_jobs'])
run=read(B/'final-ci-133b/run.json.gz');assert run['headSha']==recorded and run['status']=='completed' and run['conclusion']=='success'
assert sha(ROOT/'tests/oracle/clojure-test-suite-observations.json')=='449264ab6bca1dcf761d0204e4a64de4de91562e576615c4e84a63246839f4c7'
assert sha(ROOT/'tests/oracle/clojure-test-suite-known-failures.json')=='dea5db93e9548008d91e5c79ee11978920575975ec601f1852139ff063329d7f'
assert sha(ROOT/'scripts/clojure_test_suite.py')=='197166b70c16a1bc5e7a82d5061b069b5ba7a45453056a014704b1c345de617b'
print('Recorded133b exactworkspace actual0/10420.338s and all10requiredCIchecks success verified. Every executed tracked code/test/fixture input is conserved; only three prose files and explicit result archives/verifier may differ. Existing ignored tests and original primary/baseline/count/skip/opaque policies remain unchanged. Independent review and CI on any later head remain separate requirements.')
