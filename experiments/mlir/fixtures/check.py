#!/usr/bin/env python3
"""Original Suss MIT/Apache-2.0 code: verify fixtures and lossless print roundtrip.
Usage: python3 fixtures/check.py /absolute/path/to/suss-mlir-opt
"""
import pathlib
import subprocess
import sys

root = pathlib.Path(__file__).parent
binary = sys.argv[1]
positive = (root / 'positive.mlir').read_text()

def run(source):
    return subprocess.run([binary, '--mlir-print-debuginfo'], input=source,
                          text=True, capture_output=True, timeout=30)

first = run(positive)
assert first.returncode == 0, first.stderr
second = run(first.stdout)
assert second.returncode == 0, second.stderr
assert first.stdout == second.stdout, 'print/parse roundtrip changed IR'
for marker in ['"capture.sus":1:9', '"capture.sus":1:15',
               '"capture.sus":1:24', '"capture.sus":2:1']:
    assert marker in first.stdout, f'lost location: {marker}'
print('PASS positive and stable complete fixture locations')

for fixture in sorted(root.glob('positive-*.mlir')):
    first = run(fixture.read_text())
    assert first.returncode == 0, f'{fixture.name}: {first.stderr[:2000]}'
    second = run(first.stdout)
    assert second.returncode == 0, f'{fixture.name}: {second.stderr[:2000]}'
    assert first.stdout == second.stdout, f'{fixture.name}: roundtrip changed IR'
    print(f'PASS {fixture.name}: verify and stable roundtrip')

for fixture in sorted(root.glob('negative-*.mlir')):
    expected = fixture.read_text().splitlines()[0].removeprefix('// expected: ')
    result = run(fixture.read_text())
    assert result.returncode != 0, f'{fixture.name}: malformed IR accepted'
    assert expected in result.stderr, f'{fixture.name}: wrong rejection: {result.stderr[:2000]}'
    print(f'PASS {fixture.name}: {expected}')

# Opting into a foreign region-bearing operation must not weaken the registered
# closure's capture verifier. This passed before recursive isolation was added.
result = subprocess.run([binary, '--allow-unregistered-dialect'],
                        input=(root / 'nested-external-value.mlir').read_text(),
                        text=True, capture_output=True, timeout=30)
assert result.returncode != 0 and 'external value' in result.stderr, result.stderr
print('PASS recursive isolation through an admitted foreign region')

# Full selected-facts storage comparison; no native HIR provenance claim.
import runpy
runpy.run_path(str(root / 'check-analysis.py'), run_name='__main__')
