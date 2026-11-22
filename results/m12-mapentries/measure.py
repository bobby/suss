"""Compare the published closure compiler with the exact vector source.

SUSS_MAPENTRY_TARGET selects the isolated Cargo target. Run after Cargo completes.
The retained source-check must pass before any component or measurement is used.
"""
from pathlib import Path
import hashlib
import json
import os
import shutil
import subprocess
import sys

root = Path(__file__).resolve().parents[2]
p = root / 'results/m12-mapentries'
outer = root.parents[2]
work = root / 'target/probe/m12-mapentries-bench'
work.mkdir(parents=True, exist_ok=True)
subprocess.run([sys.executable, str(p / 'source-check.py')], check=True)
hints = outer / 'target/probe/m12-hints-source'
for suffix in ['sus', 'wit']:
    shutil.copy2(hints / f'target/probe/m12-escape-bench/bench.{suffix}', work / f'bench.{suffix}')
    shutil.copy2(work / f'bench.{suffix}', p / f'bench.{suffix}')
before = work / 'before-suss'
after = work / 'after-suss'
shutil.copy2(outer / 'target/probe/m12-aggregate-source/target/probe/m12-inline-nth-bench/after-suss', before)
shutil.copy2(Path(os.environ['SUSS_MAPENTRY_TARGET']) / 'debug/suss', after)
sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
assert sha(before) == 'cdb3d91f8bdbec8b8255c534d1afc2e8d24d3bdf52687a618dfa89514841f6c5'
env = os.environ | {'SUSS_BUILD_DIR': str(outer / 'target/suss')}
record = {'source_inputs': 'source-inputs.json', 'before_source_commit': '9596dce64e4fd4f94054c2b78cb689f32e22d7cf', 'benchmark_sha256': sha(work / 'bench.sus'), 'wit_sha256': sha(work / 'bench.wit'), 'runs': {}}
names = ['control', 'vector', 'entry', 'closure']
for tag, binary in [('before', before), ('after', after)]:
    row = {'compiler_sha256': sha(binary), 'cases': {}}
    record['runs'][tag] = row
    command = [str(binary), 'compile', 'bench.sus', '-w', 'bench.wit', '-o', tag + '.wasm']
    for name in names:
        command += ['--export', f'case-{name}=escapebench/case-{name}']
    with (p / (tag + '-compile.log')).open('w') as log:
        result = subprocess.run(command, cwd=work, env=env, stdout=log, stderr=subprocess.STDOUT)
    row['compile_command'] = command
    row['compile_exit_code'] = result.returncode
    assert result.returncode == 0, (tag, result.returncode)
    row['component_sha256'] = sha(work / (tag + '.wasm'))
    with (p / (tag + '-runs.log')).open('w') as log:
        for name in names:
            case = {'rss': [], 'perf': []}
            row['cases'][name] = case
            base = [str(binary), 'run', tag + '.wasm', '--invoke', 'case-' + name, '--']
            for extra in [{}, {'SUSS_GC': 'null'}]:
                r = subprocess.run(base + ['10'], cwd=work, env=env | extra, capture_output=True, text=True)
                assert r.returncode == 0 and float(r.stdout) == 30000000090, (tag, name, r.stdout, r.stderr)
            for n in [1000000, 2000000, 1000000, 2000000]:
                command = base + [str(n)]
                with (work / 'peak.stdout').open('w') as output:
                    process = subprocess.Popen(command, cwd=work, env=env | {'SUSS_GC': 'null'}, stdout=output, stderr=subprocess.PIPE)
                    _, status, usage = os.wait4(process.pid, 0)
                    error = process.stderr.read().decode()
                    process.returncode = os.waitstatus_to_exitcode(status)
                output = (work / 'peak.stdout').read_text()
                assert process.returncode == 0 and float(output) == 3000000000*n + n*(n-1), (tag, name, n, process.returncode, output, error)
                item = {'count': n, 'command': command, 'status': process.returncode, 'peak_bytes': usage.ru_maxrss*1024, 'stdout': output, 'stderr': error}
                case['rss'].append(item)
                log.write(json.dumps({'case': name, 'rss': item}) + '\n'); log.flush()
            case['bytes_per_iteration'] = [(case['rss'][i+1]['peak_bytes'] - case['rss'][i]['peak_bytes']) / 1000000 for i in [0, 2]]
            for n in [100000, 200000]:
                command = ['perf', 'stat', '-x,', '-e', 'instructions:u', '--', *base, str(n)]
                r = subprocess.run(command, cwd=work, env=env, capture_output=True, text=True)
                assert r.returncode == 0 and float(r.stdout) == 3000000000*n + n*(n-1), (tag, name, n, r.returncode, r.stdout, r.stderr)
                counts = [int(line.split(',')[0]) for line in r.stderr.splitlines() if ',instructions:u,' in line]
                assert len(counts) == 1, r.stderr
                item = {'count': n, 'command': command, 'status': r.returncode, 'instructions': counts[0], 'stdout': r.stdout, 'stderr': r.stderr}
                case['perf'].append(item)
                log.write(json.dumps({'case': name, 'perf': item}) + '\n'); log.flush()
            case['instructions_per_iteration'] = (case['perf'][1]['instructions'] - case['perf'][0]['instructions']) / 100000
            (p / 'measurement.json').write_text(json.dumps(record, indent=2) + '\n')
            print(tag, name, case['bytes_per_iteration'], case['instructions_per_iteration'], flush=True)
subprocess.run([sys.executable, str(p / 'source-check.py')], check=True)
