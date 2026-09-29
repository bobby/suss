#!/usr/bin/env python3
"""Probe the candidate CLI tools without changing Cargo dependencies.

Compilation of component type declarations is deliberately reported separately
from execution. This probe alone does not establish WASI 0.3.1 interoperability.
"""
import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--wasmtime', default='wasmtime')
    parser.add_argument('--wasm-tools', default='wasm-tools')
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    version = subprocess.check_output([args.wasmtime, '--version'], text=True).strip()
    if not version.startswith('wasmtime 49.0.1 '):
        raise SystemExit(f'expected candidate Wasmtime 49.0.1, got {version}')
    wit_version = subprocess.check_output([args.wasm_tools, '--version'], text=True).strip()
    if not wit_version.startswith('wasm-tools 1.258.0'):
        raise SystemExit(f'expected candidate wasm-tools 1.258.0, got {wit_version}')
    results = []
    with tempfile.TemporaryDirectory(prefix='suss-profile-') as temp:
        for file, phase, flag in [
            ('gc', 'execute', 'gc'), ('tail-call', 'execute', 'tail-call'),
            ('function-references', 'execute', 'function-references'),
            ('exceptions', 'execute', 'exceptions'),
            ('map', 'compile-type-declarations', 'component-model-map'),
            ('future-stream', 'compile-type-declarations', 'component-model-async'),
            ('implements', 'compile-type-declarations', 'component-model-implements'),
        ]:
            source = ROOT / 'tests/toolchain' / (file + '.wat')
            command = [args.wasmtime, 'run' if phase == 'execute' else 'compile', '-W', flag + '=y']
            if phase == 'execute':
                command += ['--invoke', 'answer', str(source)]
            else:
                command += [str(source), '-o', str(Path(temp) / (file + '.cwasm'))]
            result = subprocess.run(command, capture_output=True, text=True, timeout=30)
            passed = result.returncode == 0 and (phase != 'execute' or result.stdout.strip() == '42')
            results.append({'probe': file, 'phase': phase, 'passed': passed,
                            'source_sha256': hashlib.sha256(source.read_bytes()).hexdigest(),
                            'stdout': result.stdout.strip(), 'stderr': result.stderr.strip()})
        source = ROOT / 'tests/toolchain/profile.wit'
        result = subprocess.run([args.wasm_tools, 'component', 'wit', str(source)],
                                capture_output=True, text=True, timeout=30)
        results.append({'probe': 'wit-map-async-external-id', 'phase': 'parse-and-print-wit',
                        'passed': result.returncode == 0 and all(token in result.stdout for token in
                            ['map<string, u32>', 'async func', 'future<result>', 'stream<u8>', '@external-id']),
                        'source_sha256': hashlib.sha256(source.read_bytes()).hexdigest(),
                        'stdout': result.stdout.strip(), 'stderr': result.stderr.strip()})
    evidence = {'schema': 1, 'wasmtime': version, 'wasm-tools': wit_version, 'scope': 'candidate CLI; production Cargo dependencies unchanged',
                'probes': results, 'remaining': [
                    'Official WASI 0.3.1 package graph lock with content hashes',
                    'Async canonical lift/lower, callbacks, cancellation and resource transfers',
                    'Map values passed bidirectionally (not just type declarations)',
                    'Candidate Rust dependency migration and shared-runtime probe on that engine',
                    'Browser runtime and async bridge',
                ]}
    output = json.dumps(evidence, indent=2) + '\n'
    if args.output:
        args.output.write_text(output)
    else:
        print(output, end='')
    if not all(r['passed'] for r in results):
        raise SystemExit(1)


if __name__ == '__main__':
    main()
