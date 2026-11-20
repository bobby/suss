#!/usr/bin/env python3
"""Program instructions only, using the unchanged bench.py expressions/loops."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import statistics
import subprocess
import sys

source, executable, work = map(lambda p: Path(p).resolve(), sys.argv[1:4])
work.mkdir(parents=True, exist_ok=False)
script = source / 'experiments/numeric-representation/suss-jvm/bench.py'
sys.argv = [str(script), '5000000', '3']
spec = importlib.util.spec_from_file_location('bench', script)
bench = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bench)
cases = [('warm', 'i', 1000000)] + [(name, expr, 1000) for name, expr in bench.PROGRAMS]
forms = ['(ns bench)', '(def sentinel -1)'] + [f'(def {name} {init})' for name, init, _ in bench.SETUP] + bench.DEFS
forms += [f'(def case-{name} (fn [n] {bench.loop(name, expr, "n")}))' for name, expr, _ in cases]
(work / 'bench.sus').write_text('\n'.join(forms) + '\n')
exports = ' '.join(f'export case-{name}: func(n: u32) -> u32;' for name, _, _ in cases)
(work / 'bench.wit').write_text(f'package bench:programs; world bench {{ {exports} }}\n')
command = [str(executable), '--build-dir', str(work / 'build'), 'compile', 'bench.sus', '-w', 'bench.wit', '-o', 'bench.wasm']
for name, _, _ in cases:
    command += ['--export', f'case-{name}=bench/case-{name}']
r = subprocess.run(command, cwd=work, capture_output=True, text=True)
(work / 'compile.stdout').write_text(r.stdout)
(work / 'compile.stderr').write_text(r.stderr)
r.check_returncode()
def measure(name, n):
    r = subprocess.run(['perf', 'stat', '-x,', '-e', 'instructions:u', '--', str(executable), 'run', 'bench.wasm', '--invoke', f'case-{name}', '--', str(n)], cwd=work, capture_output=True, text=True, check=True)
    counts = [int(line.split(',')[0]) for line in r.stderr.splitlines() if 'instructions' in line and line.split(',')[0].isdigit()]
    if len(counts) != 1:
        raise RuntimeError(r.stderr)
    return dict(n=n, instructions=counts[0], stdout=r.stdout, stderr=r.stderr)
result = dict(source_head=subprocess.check_output(['git', '-C', str(source), 'rev-parse', 'HEAD'], text=True).strip(),
              executable=str(executable), sha256=hashlib.sha256(executable.read_bytes()).hexdigest(),
              bench_sha256=hashlib.sha256(script.read_bytes()).hexdigest(), cases=cases,
              warmup=measure('warm', 0), raw={}, per_run={}, load_start=os.getloadavg())
for name, expr, n in cases:
    observations = [measure(name, count) for _ in range(3) for count in [0, n]]
    result['raw'][name] = observations
    count = (statistics.median(r['instructions'] for r in observations if r['n'] == n) - statistics.median(r['instructions'] for r in observations if r['n'] == 0)) / n
    result['per_run'][name] = count
    (work / 'instructions.json').write_text(json.dumps(result, indent=2) + '\n')
    print(name, count, 'instructions per loop iteration; no walltime measurement', flush=True)
result['load_end'] = os.getloadavg()
(work / 'instructions.json').write_text(json.dumps(result, indent=2) + '\n')
