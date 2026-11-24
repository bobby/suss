"""Execute the explicitly ignored optimizer guard against the frozen source."""
from pathlib import Path
import gzip, hashlib, json, os, signal, subprocess, sys, time
signal.signal(signal.SIGHUP, signal.SIG_IGN)
root = Path(sys.argv[1]).resolve()
out = root / 'results/m12-random/optimized-after-final-baseline'
out.mkdir(exist_ok=False)
scope = os.environ['SUSS_DURABLE_SCOPE']
assert scope in Path('/proc/self/cgroup').read_text()
sha = lambda raw: hashlib.sha256(raw).hexdigest()
tool = json.loads((root / 'results/m12-random/binaryen-tool.json').read_text())
directory = Path(tool['directory'])
for name, expected in tool['verified_tool_sha256'].items():
    assert sha((directory / name).read_bytes()) == expected
paths = subprocess.check_output(['git', 'ls-files', '-z'], cwd=root).decode().split('\0')
inputs = {name: sha((root / name).read_bytes()) for name in paths if name and (root / name).is_file()}
command = ['cargo', 'test', '--workspace', '--locked', '--test', 'portable_random',
           'optimized_selected_random_import_preserves_draws', '--', '--include-ignored',
           '--nocapture', '--test-threads=2']
record = {'head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip(),
          'command': command, 'inputs': inputs, 'runner_sha256': sha(Path(__file__).read_bytes()),
          'scope': scope, 'recorder_pid': os.getpid(), 'binaryen': {name: tool[name] for name in ['directory', 'version', 'verified_tool_sha256']}, 'complete': False}
def save(): (out / 'command.json').write_text(json.dumps(record, indent=2) + '\n')
save()
env = os.environ | {'CARGO_TARGET_DIR': os.environ['SUSS_RANDOM_TARGET'],
                    'CARGO_BUILD_JOBS': '1', 'RUSTUP_TOOLCHAIN': 'stable',
                    'SUSS_WASM_OPT': str(directory / 'wasm-opt'), 'BINARYEN_CORES': '1'}
start = time.time()
with (out / 'run.log').open('w') as log:
    result = subprocess.run(command, cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT)
record.update(complete=True, exit_code=result.returncode, seconds=time.time()-start,
              inputs_unchanged=all((root / name).is_file() and sha((root / name).read_bytes()) == value
                                   for name, value in inputs.items()),
              head_unchanged=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip() == record['head'],
              binaryen_unchanged=all(sha((directory / name).read_bytes()) == value
                                     for name, value in tool['verified_tool_sha256'].items()))
save()
for name in ['command.json', 'run.log']:
    (out / (name+'.gz')).write_bytes(gzip.compress((out / name).read_bytes(), mtime=0))
with (out / 'recorder.log').open('w') as log:
    print({key: value for key, value in record.items() if key != 'inputs'}, file=log)
raise SystemExit(result.returncode)
