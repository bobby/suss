"""Link the frozen compiler library without touching a running Cargo build."""
from pathlib import Path
import hashlib, json, os, subprocess, time
ROOT = Path(__file__).resolve().parents[2]
OUT = Path(__file__).resolve().parent
TARGET = ROOT.parent / 'm12-decimal-target'
libs = list((TARGET / 'debug/deps').glob('libsuss_compile-*.rlib'))
assert len(libs) == 1, libs
record = {'base_head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
          'library': str(libs[0]), 'library_sha256': hashlib.sha256(libs[0].read_bytes()).hexdigest(),
          'probe_sha256': hashlib.sha256((OUT/'probe.rs').read_bytes()).hexdigest(), 'commands': []}
def run(command):
    started = time.time()
    r = subprocess.run(list(map(str, command)), cwd=ROOT, text=True, capture_output=True,
                       env=os.environ | {'XDG_RUNTIME_DIR': '/run/user/1000'})
    record['commands'].append({'command': list(map(str, command)), 'exit_code': r.returncode,
                              'seconds': time.time()-started, 'stdout': r.stdout, 'stderr': r.stderr})
    (OUT/'profile.json').write_text(json.dumps(record, indent=2)+'\n')
    print(r.stdout+r.stderr, flush=True)
    assert r.returncode == 0
run(['rustup', 'run', 'stable', 'rustc', '--edition=2024', '-C', 'opt-level=1',
     OUT/'probe.rs', '--extern', 'suss_compile='+str(libs[0]),
     '-L', 'dependency='+str(TARGET/'debug/deps'), '-o', OUT/'probe'])
record['executable_sha256'] = hashlib.sha256((OUT/'probe').read_bytes()).hexdigest()
for fuel in [1_000_000_000, 100_000_000]:
    run(['systemd-run', '--user', '--scope', '-p', 'MemoryMax=3G', '--', OUT/'probe', fuel])
record['complete'] = True
(OUT/'profile.json').write_text(json.dumps(record, indent=2)+'\n')
