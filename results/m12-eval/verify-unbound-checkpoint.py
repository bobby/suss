"""Conserve the executed binding fix and its actual negative/positive terminals."""
from pathlib import Path
import gzip,hashlib,json,re,subprocess,tarfile

ROOT=Path(__file__).resolve().parents[2];BASE=Path(__file__).resolve().parent
sha=lambda raw:hashlib.sha256(raw).hexdigest()
def read(path):return gzip.decompress(path.read_bytes())
def receipt(folder):
    path=BASE/folder/'commands.json.gz';r=json.loads(read(path))
    assert r['complete'] and r['inputs_unchanged'] and r['head_unchanged'] and r['binaryen_unchanged']
    assert all(x['complete'] and x['exit_code']==0 for x in r['commands'])
    assert sha((path.parent/'run.py').read_bytes())==r['runner_sha256']
    return r

focused=receipt('unbound-dynamic-focused');post=receipt('unbound-dynamic-read-instrumentation')
assert len(focused['commands'])==5 and len(post['commands'])==2
prose={'results/m12-eval/README.md','docs/compatibility/README.md','docs/roadmap/handoff.md'}
instrumentation={'crates/suss-compile/tests/portable_closures.rs','crates/suss-compile/tests/portable_resolution.rs','crates/suss-compile/tests/portable_dynamic_bindings.rs','results/m12-eval/verify-live-vars.py','results/m12-eval/live-vars-source-provenance.json'}
for r,allowed in [(focused,prose|instrumentation),(post,prose)]:
    assert all((ROOT/p).is_file() and sha((ROOT/p).read_bytes())==h for p,h in r['inputs'].items() if p not in allowed),'executed source/test/primary inputs changed'
proof=json.loads((BASE/'unbound-dynamic-read-instrumentation/unbound-instrumentation-source-conservation.json').read_bytes())
assert set(proof['allowed_only_instrumentation_and_provenance_changes'])==instrumentation
assert all(sha((ROOT/p).read_bytes())==h for p,h in proof['files'].items())
assert '61 passed; 0 failed' in read(BASE/'unbound-dynamic-focused/complete-runtime-abi.log.gz').decode()
assert '2 passed; 0 failed' in read(BASE/'unbound-dynamic-focused/unchanged-publication-regression.log.gz').decode()
assert '42 passed; 0 failed' in read(BASE/'unbound-dynamic-focused/complete-public-eval.log.gz').decode()
host=read(BASE/'unbound-dynamic-focused/host-library-regressions.log.gz').decode()
assert '118 passed; 0 failed' in host
rows=re.findall(r'context-budget-ledger (Runtime|Macro) elements=(\d+) snapshot=(\d+) child=(\d+) public=(\d+) lower-bound=(\d+)',host)
assert len(rows)==4 and all(int(public)>=int(bound)==int(snapshot)+int(child) for _,_,snapshot,child,public,bound in rows)
log=read(BASE/'unbound-dynamic-read-instrumentation/complete-source-order-and-binding-targets.log.gz').decode()
assert len(re.findall(r'test result: ok\. \d+ passed; 0 failed',log))==3
before=BASE/'unbound-frame-snapshot-before';negative=json.loads(read(before/'command.json.gz'))
assert negative['exit_code']==101 and negative['binary_unchanged'] and negative['head_unchanged']
assert negative['head']=='4003dc28a054d5e10d4009a15fcc819cebe0dab3'
assert 'FAILED. 0 passed; 1 failed' in read(before/'stdout.gz').decode()
with tarfile.open(before/'source-before.tar.gz') as archive:
    for member in archive.getmembers():
        raw=archive.extractfile(member).read()
        assert raw==subprocess.check_output(['git','show',negative['head']+':'+member.name],cwd=ROOT)
source='crates/suss-cli/tests/compiled_macro_publication_cells.rs'
assert (ROOT/source).read_bytes()==subprocess.check_output(['git','show',negative['head']+':'+source],cwd=ROOT)
interrupted=json.loads(read(BASE/'full-workspace-after-context-fuel-fix/command.json.gz'))
assert interrupted['complete'] and interrupted['exit_code']==101 and interrupted['inputs_unchanged'] and interrupted['head_unchanged']
assert sha((ROOT/'tests/oracle/clojure-test-suite-observations.json').read_bytes())=='449264ab6bca1dcf761d0204e4a64de4de91562e576615c4e84a63246839f4c7'
assert sha((ROOT/'tests/oracle/clojure-test-suite-known-failures.json').read_bytes())=='dea5db93e9548008d91e5c79ee11978920575975ec601f1852139ff063329d7f'
print('Executed binding fix conserved:61ABI/2unchanged-publication/42eval/118host and three complete source-order targets pass. Real before101, interrupted workspace101 and unchanged primary/baseline retained. Broader/full/review/CI remain separate gates.')
