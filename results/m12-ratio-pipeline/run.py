"""Run the entire affected target with frozen tracked inputs and exact receipts."""
from pathlib import Path
import hashlib, json, os, subprocess, time
ROOT = Path(__file__).resolve().parents[2]
OUT = Path(__file__).resolve().parent
command = ['cargo', 'test', '--locked', '-p', 'suss-compile', '--test', 'portable_pipeline', '--', '--test-threads=2']
env = os.environ | {'CARGO_TARGET_DIR': str(ROOT.parent / 'm12-decimal-target'),
                    'CARGO_BUILD_JOBS': '2', 'RUSTUP_TOOLCHAIN': 'stable',
                    'XDG_RUNTIME_DIR': '/run/user/1000'}
paths = subprocess.check_output(['git', 'ls-files', '-z'], cwd=ROOT).decode().split('\0')
inputs = {p: hashlib.sha256((ROOT / p).read_bytes()).hexdigest()
          for p in paths if p and not p.startswith('results/') and (ROOT / p).is_file()}
record = {'head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
          'command': command, 'environment': {k: env[k] for k in ['CARGO_TARGET_DIR', 'CARGO_BUILD_JOBS', 'RUSTUP_TOOLCHAIN']},
          'scope': 'MemoryMax=4G', 'complete': False, 'inputs': inputs}
def save():
    (OUT / 'command.json').write_text(json.dumps(record, indent=2) + '\n')
save()
started = time.time()
with (OUT / 'run.log').open('w') as log:
    r = subprocess.run(['systemd-run', '--user', '--scope', '-p', 'MemoryMax=4G', '--', *command],
                       cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT)
record.update(exit_code=r.returncode, seconds=time.time() - started, complete=True,
              inputs_unchanged=all(hashlib.sha256((ROOT / p).read_bytes()).hexdigest() == h for p, h in inputs.items()))
save()
print({k: v for k, v in record.items() if k != 'inputs'}, flush=True)
