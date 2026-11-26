"""Verify actual fa0da local terminals and exact executed source after publication."""
from pathlib import Path
import gzip,hashlib,json,subprocess
ROOT=Path(__file__).resolve().parents[2];B=Path(__file__).resolve().parent
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
read=lambda p:json.loads(p.read_bytes())
archived=lambda path:json.loads(gzip.decompress(path.read_bytes()))
record=read(B/'macro-flag-opcode-checkpoint.json');code=record['code_head']
assert code=='fa0da7ec4ce9e37ef5493f1fc99d652b0f2d25be'
prose={'results/m12-eval/README.md','docs/compatibility/README.md','docs/roadmap/handoff.md'}
assert set(record['prose'])==prose
for name in ['published_artifacts','prose']:
 for path,digest in record[name].items():assert sha(ROOT/path)==digest,path
covered=set()
for directory,count in [('macro-flag-opcode-focused',6),('macro-flag-opcode-broader',11)]:
 r=archived(B/directory/'commands.json.gz')
 assert r['head']==code and len(r['commands'])==count
 assert all(r[k] for k in ['complete','inputs_unchanged','head_unchanged','binaryen_unchanged'])
 assert all(c['complete'] and c['exit_code']==0 for c in r['commands'])
 assert sha(B/directory/'run.py')==r['runner_sha256']
 covered.update(r['inputs'])
 for path,digest in r['inputs'].items():
  if path not in prose:assert sha(ROOT/path)==digest,path
 if count==11:assert r['required_workspace_passed'] and r['actual_workspace_exit_code']==0 and r['external_tools_unchanged']
full=archived(B/'full-workspace-after-macro-flag-opcode/command.json.gz')
assert full['head']==code and full['complete'] and full['exit_code']==0 and full['head_unchanged'] and full['inputs_unchanged']
assert full['command']==['cargo','test','--workspace','--locked','--','--test-threads=2']
for path,digest in full['inputs'].items():
 if path not in prose:assert sha(ROOT/path)==digest,path
current=subprocess.check_output(['git','ls-files','-co','--exclude-standard'],cwd=ROOT,text=True).splitlines()
for path in current:
 if not path.startswith('results/') and (ROOT/path).is_file():assert path in covered,path
head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
if head!=code:
 subprocess.run(['git','merge-base','--is-ancestor',code,head],cwd=ROOT,check=True)
 changed=set(subprocess.check_output(['git','diff','--name-only',code,head],cwd=ROOT,text=True).splitlines())
 allowed=set(record['published_artifacts'])|prose|{'results/m12-eval/macro-flag-opcode-checkpoint.json'}
 assert changed<=allowed,sorted(changed-allowed)
assert sha(ROOT/'tests/oracle/clojure-test-suite-observations.json')=='449264ab6bca1dcf761d0204e4a64de4de91562e576615c4e84a63246839f4c7'
assert sha(ROOT/'tests/oracle/clojure-test-suite-known-failures.json')=='dea5db93e9548008d91e5c79ee11978920575975ec601f1852139ff063329d7f'
assert sha(ROOT/'scripts/clojure_test_suite.py')=='197166b70c16a1bc5e7a82d5061b069b5ba7a45453056a014704b1c345de617b'
print('Actualfa0da focused336/bootstrap/native5564/strict/exactworkspace0 conserved across explicit prose/evidence publication. Current independent scope review and final-head CI remain separate requirements.')
