"""Apply only the two independently measured root-support diagnostic advances."""
from pathlib import Path
import copy,gzip,json
ROOT=Path(__file__).resolve().parents[2]

def advance_root_diagnostics(ledger):
    old=json.loads(gzip.decompress((ROOT/'results/m12-eval/root-metadata-native-diagnostic-checkpoint/known-failures-before.json.gz').read_bytes()))
    new=json.loads(gzip.decompress((ROOT/'results/m12-eval/root-diagnostic-native-checkpoint/source-overlay/tests/oracle/clojure-test-suite-known-failures.json.gz').read_bytes()))
    review=json.loads((ROOT/'results/m12-eval/root-diagnostic-native-checkpoint/review.json').read_bytes())
    assert review['actual_native_exit_code']==0 and review['observed_row_decisions_preserved']==5564
    expected=copy.deepcopy(ledger)
    for namespace in ['clojure.core-test.add-watch','clojure.core-test.remove-watch']:
        before=old['namespaces'][namespace];after=new['namespaces'][namespace]
        assert ledger['namespaces'][namespace]==before,namespace
        assert before['stage']==after['stage']=='namespace'
        assert after['diagnostic']==before['diagnostic'].replace('Unresolved Runtime name alter-var-root at bytes','Unresolved Runtime name ref at bytes')
        assert after!=before
        expected['namespaces'][namespace]=after
    return expected
