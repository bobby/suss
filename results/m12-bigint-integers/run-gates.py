"""Run source-frozen integer feature, ABI, map and bootstrap checks."""
from pathlib import Path
import hashlib
import json
import os
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]
OUT = Path(__file__).resolve().parent
ENV = os.environ | {
    'CARGO_TARGET_DIR': str(ROOT.parents[2] / 'target/probe/m12-integer-target'),
    'CARGO_BUILD_JOBS': '2', 'RUSTUP_TOOLCHAIN': 'stable',
    'XDG_RUNTIME_DIR': '/run/user/1000',
}

def cargo(package, *targets):
    command = ['cargo', 'test', '-p', package, '--locked']
    for target in targets:
        assert (ROOT / 'crates' / package / 'tests' / (target + '.rs')).is_file(), target
        command += ['--test', target]
    return command + ['--', '--test-threads=2']

COMMANDS = [
    ('focused-final', cargo('suss-cli', 'clojure_bigint_integers', 'clojure_bigints', 'read_string', 'clojure_longs_doubles')),
    ('numeric-controls', cargo('suss-cli', 'clojure_equality_bitwise', 'clojure_unboxed_locals', 'clojure_local_numeric_hints', 'clojure_raw_numeric_core', 'clojure_raw_bits_and_casts', 'clojure_numeric_extrema')),
    ('abi-controls', cargo('suss-compile', 'runtime_abi', 'portable_calling_convention', 'portable_comparator_foundations')),
    ('map-controls', cargo('suss-cli', 'map_lookups', 'portable_hash_map_sharing', 'compiled_macro_hash_maps', 'compiled_macro_maps')),
    ('bootstrap', ['sh', 'scripts/verify-bootstrap.sh']),
    ('core-import', ['python3', 'scripts/core_import.py', '--check']),
    ('numeric-helper', ['python3', 'scripts/numeric_runtime.py', '--check']),
]
files = subprocess.check_output(['git', 'diff', '--name-only'], cwd=ROOT, text=True).splitlines()
files += ['crates/suss-cli/tests/clojure_bigint_integers.rs', 'results/m12-bigint-integers/cases.json']
inputs = {f: hashlib.sha256((ROOT / f).read_bytes()).hexdigest() for f in files}
record = {'inputs': inputs, 'commands': [], 'complete': False}

def save():
    (OUT / 'commands.json').write_text(json.dumps(record, indent=2) + '\n')

save()
for label, command in COMMANDS:
    started = time.time()
    with (OUT / (label + '.log')).open('w') as log:
        result = subprocess.run(
            ['systemd-run', '--user', '--scope', '-p', 'MemoryMax=4G', '--', *command],
            cwd=ROOT, env=ENV, stdout=log, stderr=subprocess.STDOUT)
    record['commands'].append({'label': label, 'command': command,
        'exit_code': result.returncode, 'seconds': time.time() - started})
    save()
    print(label, result.returncode, flush=True)
    if result.returncode:
        raise SystemExit(result.returncode)
assert all(hashlib.sha256((ROOT / f).read_bytes()).hexdigest() == h for f, h in inputs.items())
record['complete'] = True
save()
