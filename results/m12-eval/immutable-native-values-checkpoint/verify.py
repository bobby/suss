#!/usr/bin/env python3
"""Verify exact historical receipts; --current also checks the working tree."""
from pathlib import Path
import gzip, hashlib, json, re, subprocess, sys

here = Path(__file__).resolve().parent
root = here.parents[2]
sha = lambda data: hashlib.sha256(data).hexdigest()
summary = json.loads((here / 'summary.json').read_text())
for name, digest in summary['archive_hashes'].items():
    assert sha((here / name).read_bytes()) == digest, name
states = {}
for phase, result in summary['phases'].items():
    state = json.loads(gzip.decompress((here / phase / 'commands.json.gz').read_bytes()))
    states[phase] = state
    assert state['complete'] and state['inputs_unchanged'] and state['binaryen_unchanged'], phase
    assert sha(gzip.decompress((here / phase / 'runner.py.gz').read_bytes())) == state['runner_sha256']
    assert all(row['complete'] for row in state['commands']), phase
    assert {row['name']: row['exit_code'] for row in state['commands']} == result['exits']
    for row in state['commands']:
        log = gzip.decompress((here / phase / (row['name'] + '.log.gz')).read_bytes()).decode()
        count = sum(map(int, re.findall(r'test result: ok\. (\d+) passed; 0 failed;', log)))
        assert count == result['passing_target_counts'][row['name']], (phase, row['name'])
    for name, digest in state['inputs'].items():
        overlay = here / phase / 'overlay' / (name + '.gz')
        shared = here / 'inputs' / (digest + '.gz')
        if overlay.exists() or shared.exists():
            data = gzip.decompress((overlay if overlay.exists() else shared).read_bytes())
        elif (root / name).is_file() and sha((root / name).read_bytes()) == digest:
            continue
        else:
            data = subprocess.check_output(['git', 'show', state['head'] + ':' + name], cwd=root)
        assert sha(data) == digest, (phase, name)
assert list(summary['phases']['before']['exits'].values()) == [101]
assert all(code == 0 for code in summary['phases']['focused']['exits'].values())
assert {name: code for name, code in summary['phases']['complete-controls']['exits'].items() if code} == {'full20-source-values': 101}
assert sum(summary['phases']['complete-controls']['passing_target_counts'].values()) == summary['current_distinct_passing_targets'] == 198
log = gzip.decompress((here / 'complete-controls/full20-source-values.log.gz').read_bytes()).decode()
assert {phase: len(re.findall('isolated-source-primary-match ' + phase + ' ', log)) for phase in ['Runtime', 'Macro']} == {'Runtime': 19, 'Macro': 19}
assert 'Unresolved Macro name ns-interns' in log
primary = root / 'results/m12-eval/immutable-native-graph-primary'
receipt = json.loads((primary / 'command.json').read_text())
assert receipt['exit_code'] == 0
for name, digest in receipt['files'].items():
    assert sha((primary / name).read_bytes()) == digest, name
assert (primary / 'primary.stdout').read_text().count('[true true true true]') == 9
if '--current' in sys.argv:
    for name, digest in states['complete-controls']['inputs'].items():
        if name.startswith('results/'):
            continue
        expected = summary.get('publication_prose_delta', {}).get(name, {}).get('published', digest)
        assert sha((root / name).read_bytes()) == expected, name
print('Verified exact negative/working/current receipts, 198 current distinct passing targets, nine primary controls and unchanged 19/20 per phase. The catalog aggregate remains failing; no full issue or promotion claim.')
