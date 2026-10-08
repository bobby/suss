#!/usr/bin/env python3
"""Bounded exporter gate only; never invokes Cargo or Wasmtime."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile

exe = sys.argv[1]
root = Path(__file__).parent
source = (root / 'producer-caller.mlir').read_text()
with tempfile.TemporaryDirectory(prefix='suss-export-check-') as scratch:
    scratch = Path(scratch)
    def run(text, expected='4022000000000000', success=True):
        inp, out = scratch / 'input.mlir', scratch / 'output.json'
        inp.write_text(text)
        out.write_text('sentinel')
        result = subprocess.run([exe, str(inp), '--expected-bits='+expected,
                                 '--export-graph='+str(out)], capture_output=True, text=True, timeout=20)
        if success:
            assert result.returncode == 0, result.stderr
            return json.loads(out.read_text())
        assert result.returncode != 0, 'unsupported fixture unexpectedly exported'
        assert out.read_text() == 'sentinel', 'failed export modified output'
    original = run(source)
    assert original == json.loads((root.parent / 'sample.json').read_text())
    located = run(source.replace("{value = 7.0 : f64} : () -> f64",
                                '{value = 7.0 : f64} : () -> f64 loc("genuine.sus":8:19)'))
    assert located == original, "v1 location omission policy changed"
    mutated = run((root / 'producer-caller-mutated.mlir').read_text(), '402a000000000000')
    expected = json.loads(json.dumps(original))
    expected['producer']['operations'][0]['bits'] = '4026000000000000'
    expected['expected_bits'] = '402a000000000000'
    assert mutated == expected
    # Exact storage transport witnesses; these are not arithmetic result claims.
    for literal, bits in [('0x8000000000000000','8000000000000000'),
                          ('0x7FF0000000000000','7ff0000000000000'),
                          ('0x7FF8000000000042','7ff8000000000042')]:
        graph = run(source.replace('7.0 : f64', literal+' : f64'))
        assert graph['producer']['operations'][0]['bits'] == bits
    negatives = [
        source.replace('value = 7.0 : f64', 'value = 7.0 : f64, suss.analysis = {binding = 1 : i64}'),
        source.replace('"suss.add"', '"suss.unknown"'),
        source.replace('%captured, %arg', '%x, %arg'),
        source.replace('value = 7.0 : f64', 'value = 7.0 : f32'),
        source.replace('module {', 'module attributes {extra = true} {'),
        source.replace('%binding, %n', '%binding'),
        source.replace('"suss.return"(%sum)', '"suss.return"(%f)'),
    ]
    for text in negatives: run(text, success=False)
    run((root / "negative-numeric-producer.mlir").read_text(), success=False)
    run(source, '402200000000000A', success=False)
print('PASS: exact original/mutated export, explicit location omission, 3 raw float bit probes, 9 rejection probes; Wasmtime pending')
