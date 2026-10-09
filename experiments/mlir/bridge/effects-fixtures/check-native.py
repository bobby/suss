#!/usr/bin/env python3
"""Compare actual Wasmtime observations against separate source-oracle results."""
import json
from pathlib import Path
import subprocess
import sys

root = Path(__file__).parent
expected = json.loads((root / 'expected.json').read_text())
assert len(expected) == 7
for name, case in expected.items():
    result = subprocess.run([sys.argv[1], '--effects', str(root / (name + '.json'))],
                            capture_output=True, text=True, timeout=60, check=True)
    actual = json.loads(result.stdout)
    wanted = {
        'schema': 'suss.mlir.effects.observation.v1',
        'abi_initializer_gate': True,
        'result_bits': case['result_bits'],
        'cells': [
            {'namespace': 'effects', 'name': 'journal', 'bits': case['journal_bits']},
            {'namespace': 'effects', 'name': 'count', 'bits': case['count_bits']}
        ]
    }
    assert actual == wanted, (name, actual, wanted)
    print('PASS actual Wasmtime result and ordered cells:', name)
