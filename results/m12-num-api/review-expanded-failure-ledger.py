"""Verify the precise failure-ledger expansion; never rewrite comparison rules."""
from pathlib import Path
import gzip,hashlib,json,subprocess,sys
ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'scripts'))
import clojure_test_suite as suite
sha=lambda raw:hashlib.sha256(raw).hexdigest()
def archived(path):
    return path.read_bytes() if path.exists() else gzip.decompress(path.with_name(path.name+'.gz').read_bytes())
def main():
    out=ROOT/'results/m12-num-api/full-suite-after-source-inline'
    primary=(ROOT/'tests/oracle/clojure-test-suite-observations.json').read_bytes()
    assert sha(primary)=='449264ab6bca1dcf761d0204e4a64de4de91562e576615c4e84a63246839f4c7'
    assert sha((ROOT/'scripts/clojure_test_suite.py').read_bytes())=='197166b70c16a1bc5e7a82d5061b069b5ba7a45453056a014704b1c345de617b'
    # The historical101 is immutable; its full conservation audit still uses
    # the exact historical baseline, even in a clean checkout with gzip only.
    conservation=subprocess.run([sys.executable,str(out.parent/'review-full-capture-conservation.py'),str(out)],cwd=ROOT,capture_output=True,text=True)
    assert conservation.returncode==0,conservation.stderr
    preserved=json.loads(conservation.stdout)
    assert preserved['actual_native_exit']==101 and preserved['baseline_written'] is False
    before=json.loads(gzip.decompress((out/'baseline-before-expansion.json.gz').read_bytes()))
    observed=json.loads(archived(out/'capture/suite-capture/suite-observations.json'))
    counts,after=suite.compare(json.loads(primary),observed)
    assert json.loads((ROOT/'tests/oracle/clojure-test-suite-known-failures.json').read_bytes())==after
    assert before['tests']==after['tests'] and before['skip-mismatches']==after['skip-mismatches']==[]
    added={f'clojure.core-test.num/test-num#{n}':('count:26/18' if n<=18 else 'unexpected-assertion') for n in range(1,27)}
    added['clojure.core-test.remove/test-remove#18']='verdict:fail'
    assert after['assertions']=={**before['assertions'],**added}
    loaded={'clojure.core-test.'+s for s in ['eq','not-eq','num','remove']}
    advanced={'clojure.core-test.nth':'re-matcher','clojure.core-test.repeatedly':'repeatedly','clojure.core-test.random-uuid':'clojure.string','clojure.string-test.replace':'clojure.string'}
    assert set(before['namespaces'])-set(after['namespaces'])==loaded
    for ns,row in before['namespaces'].items():
        if ns in loaded:continue
        if ns in advanced:
            assert row['stage']=='read' and after['namespaces'][ns]['stage']=='namespace'
            assert advanced[ns] in after['namespaces'][ns]['diagnostic']
        else:assert after['namespaces'][ns]==row,ns
    assert set(after['namespaces'])==set(before['namespaces'])-loaded
    # Independently compare the exact newly executed26-row body against its
    # actual pinned Clojure/default primary, preserving all three failed JVM
    # narrow-wrapper verdicts. The global JVM/clj18 primary stays untouched.
    source=ROOT/'results/m12-numeric-source-bodies/num-selected-primary'
    build=json.loads((source/'reference-build.json').read_bytes())
    raw=(source/'reference.json').read_bytes();assert sha(raw)==build['output_sha256']
    assert build['runs']==4 and build['primary_exit_codes']==[1]*4 and build['preserved_primary_failures']==[7,8,9]
    default=json.loads(raw);test='clojure.core-test.num/test-num'
    selected={'schema':1,'suite':observed['suite'],'assertions':[r for r in observed['assertions'] if r['test']==test],
              'namespaces':[r for r in observed['namespaces'] if r['namespace']=='clojure.core-test.num'],'test-failures':[],'skips':[]}
    assert [(r['test'],r['ordinal']) for r in selected['assertions']]==[(test,n) for n in range(1,27)]
    dc,df=suite.compare(default,selected)
    assert dc=={'pass':23,'guest-judged':0,'fail':3,'not-executed':0,'skipped':0,'namespaces-failed':0}
    assert set(df['assertions'])=={f'{test}#{n}' for n in [7,8,9]}
    for n,kind in [(7,'short'),(8,'byte'),(9,'int')]:
        expected=default['assertions'][n-1];actual=selected['assertions'][n-1]
        assert expected['value']=={'tag':'bool','value':False} and expected['verdict']=='fail'
        assert actual['value']=={'tag':'bool','value':True} and actual['verdict']=='pass'
        assert f'({kind} 1)' in expected['form']
    remove=next(r for r in observed['assertions'] if r['test']=='clojure.core-test.remove/test-remove' and r['ordinal']==18)
    wanted=next(r for r in json.loads(primary)['assertions'] if r['id']=='clojure.core-test.remove/test-remove#18')
    assert suite.decide(wanted,remove)[0]=='verdict:fail'
    contracts={
      'docs/adr/0003-clojure-numeric-tower.md':'0cf136ad1e030a0f4ecc9d061f485c926f8cd69fede48e06742f6f04e39ff16c',
      'docs/design/suss-0.3.1.md':'4f1a078e2f94a88e5a8779ed3fdeea78d93cbe3cdb73ceab1d8a7312262ef303',
      'vendor/clojure-test-suite/upstream/test/clojure/core_test/num.cljc':'ac6d931f0056bede47c51c484033431acf22b28945463e2a36271f2d4615dd36',
      'vendor/clojure-test-suite/upstream/test/clojure/core_test/remove.cljc':'e0e443aed3f83f024be9747f993022628a9f07958a87b3dacd484a85c984959f'}
    assert all(sha((ROOT/p).read_bytes())==h for p,h in contracts.items())
    print(json.dumps({'historical_capture_counts':counts,'preserved_numeric_tagged_host_matches':preserved['numeric_tagged_host_matches'],
        'current_ledger_sha256':sha((ROOT/'tests/oracle/clojure-test-suite-known-failures.json').read_bytes()),
        'primary_unchanged':True,'comparator_unchanged':True,'loaded_namespaces':sorted(loaded),'advanced_dependencies':advanced,
        'new_explicit_failures':added,'complete_default_num':dc,'contracts':contracts,
        'qualification':'Failure-ledger accounting only: exact count/profile and character differences remain failures, with no new skip or opaque judgment. Historical native101 is retained. This does not establish a fresh native0 on integrated source or substitute for workspace, independent review or final-head CI.'},indent=2))
if __name__=='__main__':main()
