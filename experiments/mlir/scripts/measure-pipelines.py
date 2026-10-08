#!/usr/bin/env python3
"""Same captured closure/live-cell call via native source and verified MLIR.

Time compilation/emission commands only; execute every pair outside that timing.
MLIR route includes a C++ exporter launch plus Rust bridge launch; native includes
one Rust source compiler launch. No source-to-MLIR conversion is implemented or timed.
"""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import statistics
import subprocess
import tempfile
import time

FIXTURES = Path(__file__).resolve().parents[1] / 'bridge' / 'export-fixtures'
parser = argparse.ArgumentParser()
parser.add_argument('bridge', type=Path)
parser.add_argument('exporter', type=Path)
args = parser.parse_args()

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def run(command):
    result = subprocess.run([str(arg) for arg in command], capture_output=True,
                            timeout=60, check=True)
    return result.stdout

samples = {'native': [], 'mlir': []}
artifacts = {}
with tempfile.TemporaryDirectory(prefix='suss-mlir-measure-') as directory:
    root = Path(directory)
    for mutated, expected in [(False, '4022000000000000'),
                              (True, '402a000000000000')]:
        case = 'mutated' if mutated else 'original'
        producer = root / 'producer.sus'
        producer.write_text((FIXTURES / 'native-producer.sus').read_text().replace(
            'x 7', 'x 11') if mutated else (FIXTURES / 'native-producer.sus').read_text())
        mlir = FIXTURES / ('producer-caller-mutated.mlir' if mutated
                           else 'producer-caller.mlir')
        reference = json.loads((FIXTURES / ('exported-mutated.json' if mutated
                                           else 'exported-original.json')).read_text())
        samples[case] = {'native': [], 'mlir': []}
        identities = {}
        for iteration in range(23):
            # Alternate order to avoid permanently favoring one lane's cache state.
            for lane in (['native', 'mlir'] if iteration % 2 == 0
                         else ['mlir', 'native']):
                output = root / lane
                started = time.perf_counter_ns()
                if lane == 'native':
                    run([args.bridge, '--emit-native', producer,
                         FIXTURES / 'native-caller.sus', output])
                else:
                    exported = run([args.exporter, mlir, '--expected-bits=' + expected])
                    graph = root / 'graph.json'
                    graph.write_bytes(exported)
                    run([args.bridge, '--emit-graph', graph, output])
                elapsed = time.perf_counter_ns() - started
                if lane == 'mlir' and json.loads(exported) != reference:
                    raise ValueError('exported graph changed')
                identity = {name: {'bytes': (output / name).stat().st_size,
                                   'sha256': digest(output / name)}
                            for name in ['producer.wasm', 'caller.wasm']}
                if lane in identities and identity != identities[lane]:
                    raise ValueError('nonreproducible emitted artifacts')
                identities[lane] = identity
                # Decode actual numeric ABI result, share recursive GC across the
                # independent fragments, force collection, and run manifest gate.
                execution = run([args.bridge, '--run-emitted', output, expected]).decode()
                if ('actual_bits=' + expected not in execution or
                    'abi_initializer_gate=true; shared_gc_after_collection=true' not in execution):
                    raise ValueError('missing actual execution checkpoints')
                if iteration >= 3:
                    samples[case][lane].append(elapsed)
        artifacts[case] = identities

print(json.dumps({
    'schema': 'suss.mlir.pipeline-comparison.v1',
    'platform': platform.platform(), 'machine': platform.machine(),
    'bridge': {'sha256': digest(args.bridge), 'bytes': args.bridge.stat().st_size},
    'exporter': {'sha256': digest(args.exporter), 'bytes': args.exporter.stat().st_size},
    'fixtures': {path.name: digest(path) for path in [
        FIXTURES / 'native-producer.sus', FIXTURES / 'native-caller.sus',
        FIXTURES / 'producer-caller.mlir', FIXTURES / 'producer-caller-mutated.mlir']},
    'stage': 'process launch + compilation/verification + emission + output file writes',
    'native_route': 'two genuine source fragments -> public compile_in -> WasmGC',
    'mlir_route': 'hand-authored MLIR -> C++ verified JSON -> Rust IR reconstruction -> compile_ir -> WasmGC',
    'excluded': ['toolchain/build time', 'source-to-MLIR conversion (not implemented)',
                 'Wasmtime compilation/execution/GC', 'source-analysis v2 sidecar'],
    'limitations': ['different parser inputs; same capture, arithmetic and live-cell call',
                    'native source identity annotations differ from IR identity annotations',
                    'MLIR uses two processes; native uses one',
                    'shared host not isolated; no production performance claim'],
    'warmups_per_case_lane': 3,
    'cases': {case: {lane: {'samples_ns': values,
                          'median_ns': statistics.median(values),
                          'minimum_ns': min(values), 'maximum_ns': max(values)}
                     for lane, values in lanes.items()}
              for case, lanes in samples.items() if case in ['original', 'mutated']},
    'artifacts': artifacts,
    'observations': 'all 92 fragment-pair executions matched exact 9/13 result bits after GC; ABI initializer gate passed'
}, indent=2))
