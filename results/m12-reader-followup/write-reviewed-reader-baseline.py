"""Publish only four audited namespace diagnostics; retain every failure/count."""
from pathlib import Path
import gzip, hashlib, json, sys

ROOT = Path(__file__).resolve().parents[2]
OUT = Path(__file__).resolve().parent / 'full-suite-after-inline-source-macros'
sys.path.insert(0, str(ROOT / 'scripts'))
import clojure_test_suite as suite

path = ROOT / 'tests/oracle/clojure-test-suite-known-failures.json'
old_bytes = gzip.decompress((OUT / 'reviewed-before-known-failures.json.gz').read_bytes())
old = json.loads(old_bytes)
audit = json.loads((OUT / 'suite-audit.json').read_text())
receipt = json.loads((OUT / 'suite-commands.json').read_text())
assert receipt['complete'] and receipt['inputs_unchanged']
assert receipt['head'] == '5ee039cfdb8281412fa0153c826bedb007e9a0a0'
assert audit['previously_passing_regressions'] == audit['raw_comparison_regressions'] == []
assert audit['prior_strict_passes_preserved'] == 4917
assert audit['prior_individually_comparable_passes_preserved'] == 4932
assert audit['counts'] == audit['old_counts']
assert audit['changes']['assertions'] == audit['changes']['tests'] == []
expected_names = {
    'clojure.core-test.identity', 'clojure.core-test.parse-uuid',
    'clojure.core-test.uuid-qmark', 'clojure.edn-test.read-string',
}
assert {change['id'] for change in audit['changes']['namespaces']} == expected_names
expected = json.loads(old_bytes)
for change in audit['changes']['namespaces']:
    assert expected['namespaces'][change['id']] == change['old']
    expected['namespaces'][change['id']].update(change['new'])
proposed = json.loads((OUT / 'proposed-known-failures.json').read_text())
assert proposed == expected
assert path.read_bytes() == old_bytes, 'baseline changed since the captured audit'
primary = (ROOT / 'tests/oracle/clojure-test-suite-observations.json').read_bytes()
assert hashlib.sha256(primary).hexdigest() == '449264ab6bca1dcf761d0204e4a64de4de91562e576615c4e84a63246839f4c7'
observed = json.loads((OUT / 'suite-capture/suite-observations.json').read_bytes())
counts, measured = suite.compare(json.loads(primary), observed)
assert measured == expected
assert old_bytes == (json.dumps(old, indent=1) + '\n').encode(), 'preserve existing formatting'
path.write_text(json.dumps(expected, indent=1) + '\n')
assert suite.compare(json.loads(primary), observed)[1] == json.loads(path.read_text())
result = {
    'change': 'Advance only four existing namespace diagnostics beyond UUID literal reading; retain all assertion/test failures, skip mismatches and counts',
    'old_sha256': hashlib.sha256(old_bytes).hexdigest(),
    'new_sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
    'primary_sha256': hashlib.sha256(primary).hexdigest(),
    'counts': counts, 'host_comparison_exit_code': 0,
    'original_capture_rust_exit_code': 101,
    'edn_assertions_executed': 0,
    'qualification': 'The original Rust drift failure remains intact. This separate host audit records diagnostic changes, not compatibility successes or a native rerun. EDN still requires a Macro-phase harness host-service contract.',
}
(OUT / 'reviewed-baseline-write.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps(result, indent=2))
