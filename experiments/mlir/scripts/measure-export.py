#!/usr/bin/env python3
"""Measure only the verified-export stage; native compile/runtime stay separate."""
import argparse
import hashlib
import json
import pathlib
import platform
import statistics
import subprocess
import time

parser = argparse.ArgumentParser()
parser.add_argument('exporter', type=pathlib.Path)
parser.add_argument('fixture', type=pathlib.Path)
parser.add_argument('expected_graph', type=pathlib.Path)
args = parser.parse_args()
expected = json.loads(args.expected_graph.read_text())

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def unique_object(items):
    result = {}
    for key, value in items:
        if key in result:
            raise ValueError(f'duplicate exported key: {key}')
        result[key] = value
    return result

samples = []
for index in range(23):
    started = time.perf_counter_ns()
    result = subprocess.run(
        [str(args.exporter), str(args.fixture),
         '--expected-bits=' + expected['expected_bits']],
        capture_output=True, timeout=30, check=True)
    elapsed = time.perf_counter_ns() - started
    actual = json.loads(result.stdout, object_pairs_hook=unique_object)
    if actual != expected:
        raise ValueError('verified export differs from independently reviewed graph')
    if index >= 3:
        samples.append(elapsed)

print(json.dumps({
    'schema': 'suss.mlir.export-measurement.v1',
    'stage': 'subprocess launch + MLIR parse/verify + graph export',
    'excluded': ['LLVM build', 'Rust build', 'Wasm emission', 'Wasmtime execution',
                 'JSON validation in this measurement script'],
    'platform': platform.platform(),
    'machine': platform.machine(),
    'exporter_sha256': digest(args.exporter),
    'exporter_bytes': args.exporter.stat().st_size,
    'fixture_sha256': digest(args.fixture),
    'expected_graph_sha256': digest(args.expected_graph),
    'warmups': 3,
    'samples_ns': samples,
    'median_ns': statistics.median(samples),
    'minimum_ns': min(samples),
    'maximum_ns': max(samples),
    'graph_observations': 'all 23 outputs exactly match the reviewed graph',
    'native_baseline_comparison': 'pending; this is not an end-to-end comparison'
}, indent=2))
