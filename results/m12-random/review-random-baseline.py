"""Remove only three independently executed namespace-load failures."""
from pathlib import Path
import collections
import copy
import gzip
import hashlib
import json
import sys

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
import clojure_test_suite as suite

BASELINE = ROOT / 'tests/oracle/clojure-test-suite-known-failures.json'
PRIMARY = ROOT / 'tests/oracle/clojure-test-suite-observations.json'
PREVIOUS = ROOT / 'results/m12-reader-followup/full-suite-after-shipped-min-key/suite-capture/suite-observations.json.gz'
BASELINE_SHA = 'ffc011e8aee7bb7439955a551c25ed75f83f7799092eba1305f1956de27435d0'
PRIMARY_SHA = '449264ab6bca1dcf761d0204e4a64de4de91562e576615c4e84a63246839f4c7'
PREVIOUS_SHA = 'c68e4bfd102b7492b68dcbbded2e498f708594889e31756ad3950c503efd4ecc'
BODIES = {
    'clojure.core-test.some-fn': (16, []),
    'clojure.core-test.constantly': (31, [1, 31]),
    'clojure.core-test.lazy-seq': (31, [1, 5, 9, 12, 13, 15, 17, 18, 19, 20]),
}

def sha(raw):
    return hashlib.sha256(raw).hexdigest()

def review(old_raw, primary_raw, previous_raw, observed_raw, proposed):
    assert sha(old_raw) == BASELINE_SHA, 'published parent baseline changed'
    assert sha(primary_raw) == PRIMARY_SHA, 'primary changed'
    assert sha(previous_raw) == PREVIOUS_SHA, 'parent capture changed'
    old, primary, previous, observed = map(json.loads, [old_raw, primary_raw, previous_raw, observed_raw])
    before_counts, before_failures = suite.compare(primary, previous)
    assert before_failures == old, 'parent capture does not establish baseline'
    counts, measured = suite.compare(primary, observed)
    expected = copy.deepcopy(old)
    for namespace in BODIES:
        assert expected['namespaces'].pop(namespace)['stage'] == 'namespace'
    assert measured == proposed == expected, 'unrelated failure/baseline change'
    assert [r['namespace'] for r in observed['namespaces']] == primary['namespaces'], 'namespace coverage changed'
    wanted = {r['id']: r for r in primary['assertions']}
    actual = {r['test']+'#'+str(r['ordinal']): r for r in observed['assertions']}
    assert len(actual) == len(observed['assertions']), 'duplicate assertion observation'
    # Keep every individually comparable old success, even one observed in a
    # namespace whose later load failed. A missing row cannot become success.
    for row in previous['assertions']:
        identity = row['test']+'#'+str(row['ordinal'])
        if suite.decide(wanted[identity], row)[0] is None:
            assert identity in actual and suite.decide(wanted[identity], actual[identity])[0] is None, identity
    bodies = {}
    for namespace, (total, guest_ordinals) in BODIES.items():
        assert next(r for r in observed['namespaces'] if r['namespace'] == namespace) == {'namespace': namespace, 'status': 'loaded'}
        refs = [r for r in primary['assertions'] if r['test'].split('/')[0] == namespace]
        rows = [r for r in observed['assertions'] if r['test'].split('/')[0] == namespace]
        assert len(refs) == len(rows) == total, 'complete original body required'
        assert [r['id'] for r in refs] == [r['test']+'#'+str(r['ordinal']) for r in rows], 'body order/identity changed'
        observed_guests = []
        for ref, row in zip(refs, rows):
            failure, guest = suite.decide(ref, row)
            assert failure is None, (ref['id'], failure)
            if guest:
                observed_guests.append(row['ordinal'])
        assert observed_guests == guest_ordinals, 'original guest judgments changed'
        bodies[namespace] = {'actual_rows': total, 'host_matches': total-len(guest_ordinals), 'guest_ordinals': guest_ordinals}
    expected_counts = before_counts | {'pass': before_counts['pass']+66, 'guest-judged': before_counts['guest-judged']+12, 'not-executed': before_counts['not-executed']-78, 'namespaces-failed': before_counts['namespaces-failed']-3}
    assert counts == expected_counts, 'unexpected count/skip/guest change'
    return expected, counts, bodies

def main():
    out = Path(sys.argv[1]).resolve()
    receipt = json.loads((out/'suite-commands.json').read_text())
    assert receipt['complete'] and receipt['inputs_unchanged']
    assert all((ROOT/p).is_file() and sha((ROOT/p).read_bytes()) == h for p,h in receipt['inputs'].items()), 'capture inputs changed before review'
    assert next(c for c in receipt['commands'] if c['label']=='suite-build')['exit_code'] == 0
    assert next(c for c in receipt['commands'] if c['label']=='suite-lock')['exit_code'] == 0
    assert next(c for c in receipt['commands'] if c['label']=='suite-run')['exit_code'] == 101, 'preserve original native discrepancy'
    old_raw = BASELINE.read_bytes(); observed_raw = (out/'suite-capture/suite-observations.json').read_bytes()
    expected, counts, bodies = review(old_raw, PRIMARY.read_bytes(), gzip.decompress(PREVIOUS.read_bytes()), observed_raw, json.loads((out/'proposed-known-failures.json').read_text()))
    new_raw = (json.dumps(expected, indent=1)+'\n').encode()
    report = {'old_sha256':sha(old_raw),'new_sha256':sha(new_raw),'capture_sha256':sha(observed_raw),'counts':counts,'complete_bodies':bodies,'original_native_capture_exit_code':101,'qualification':'Only the three proven namespace-load failures are removed. All previous outcomes, primary/count/skip rules and original guest judgments remain. Host review is not a native rerun or workspace pass.'}
    if '--write' in sys.argv[2:]:
        target=out/'reviewed-random-baseline-write.json'; assert not target.exists()
        (out/'parent-min-key-baseline.json.gz').write_bytes(gzip.compress(old_raw,mtime=0))
        BASELINE.write_bytes(new_raw); target.write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(report,indent=2))

if __name__ == '__main__':main()
