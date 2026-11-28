"""Require a fresh actual native0 plus preserved complete source observations."""
from pathlib import Path
import gzip,hashlib,json,subprocess,sys
ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'scripts'))
import clojure_test_suite as suite
sys.path.insert(0,str(Path(__file__).resolve().parent))
from reviewed_root_diagnostics import advance_root_diagnostics
sha=lambda raw:hashlib.sha256(raw).hexdigest()
def archived(path):
    return path.read_bytes() if path.exists() else gzip.decompress(path.with_name(path.name+'.gz').read_bytes())
def main():
    out=Path(sys.argv[1]).resolve();receipt=json.loads(archived(out/'suite-commands.json'))
    assert receipt['complete'] and receipt['inputs_unchanged']
    assert receipt['head']==subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
    assert all((ROOT/p).is_file() and sha((ROOT/p).read_bytes())==h for p,h in receipt['inputs'].items())
    assert {r['label']:r['exit_code'] for r in receipt['commands']}=={'suite-build':0,'suite-run':0,'suite-lock':0},'require actual native0, not outer producer0'
    ledger=subprocess.run([sys.executable,str(ROOT/'results/m12-num-api/review-expanded-failure-ledger.py')],capture_output=True,text=True,cwd=ROOT)
    assert ledger.returncode==0,ledger.stderr
    review=json.loads(ledger.stdout)
    original=ROOT/'results/m12-num-api/full-suite-after-source-inline'
    old=json.loads(archived(original/'capture/suite-capture/suite-observations.json'))
    raw=archived(out/'suite-capture/suite-observations.json');observed=json.loads(raw)
    primary=json.loads((ROOT/'tests/oracle/clojure-test-suite-observations.json').read_bytes())
    assert [(r['test'],r['ordinal']) for r in observed['assertions']]==[(r['test'],r['ordinal']) for r in old['assertions']]
    counts,failures=suite.compare(primary,observed)
    old_counts,old_failures=suite.compare(primary,old)
    assert counts==old_counts and failures==advance_root_diagnostics(old_failures)
    assert failures==json.loads((ROOT/'tests/oracle/clojure-test-suite-known-failures.json').read_bytes())
    refs={r['id']:r for r in primary['assertions']};actual={r['test']+'#'+str(r['ordinal']):r for r in observed['assertions']}
    preserved=0
    for row in old['assertions']:
        identity=row['test']+'#'+str(row['ordinal'])
        if identity in refs:
            decision=suite.decide(refs[identity],row)
            if decision[0] is None:
                assert suite.decide(refs[identity],actual[identity])==decision,identity
                preserved+=1
    def tagged(node,tags):
        if isinstance(node,dict):return node.get('tag') in tags or any(tagged(v,tags) for v in node.values())
        return isinstance(node,list) and any(tagged(v,tags) for v in node)
    matches={}
    for name,tags,total in [('bigint-ratio',{'bigint','biginteger','ratio'},229),('decimal',{'decimal'},200)]:
        selected=[r for r in primary['assertions'] if tagged(r,tags)]
        assert len(selected)==total and all(suite.decide(r,actual[r['id']])==(None,False) for r in selected)
        matches[name]=total
    report={'head':receipt['head'],'actual_native_exit':0,'capture_sha256':sha(raw),'ordered_observations':len(observed['assertions']),
            'counts':counts,'preserved_prior_row_decisions':preserved,'numeric_tagged_host_matches':matches,
            'baseline_sha256':review['current_ledger_sha256'],'primary_unchanged':True,'comparator_unchanged':True,
            'qualification':'Fresh integrated-source native0 against the explicit failure ledger, with exact complete observed identities, classifications and prior successful judgments preserved. Residual132 failures, profile/count differences and unexecuted dependencies remain explicit. Does not claim every suite assertion passes or replace default-num/API/AOT, workspace, independent review or final-head CI gates.'}
    dest=out/'integrated-capture-review.json';assert not dest.exists();dest.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
if __name__=='__main__':main()
