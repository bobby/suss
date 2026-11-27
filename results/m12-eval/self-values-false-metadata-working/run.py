from pathlib import Path
import hashlib
import json
import os
import signal
import subprocess
import time

base = Path(__file__).resolve().parent
root = Path('/home/bobby/.codex-workspaces/worktrees/ios-cd7ccbc9-1c87-43fe-95e7-2b6b71168849/suss/target/probe/m12-case-budget-source')
numeric = base / 'm12-numeric-completion/results/m12-num-api/fa0da-integrated-validation/commands.json'
out = root / 'results/m12-eval/self-values-false-metadata-working'
out.mkdir(exist_ok=False)
sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
assert head == '42aa08e8558c73acb7771b55f648dcfb5c6c85a6'
tracked = set(subprocess.check_output(['git', 'ls-files'], cwd=root, text=True).splitlines())
paths = set(subprocess.check_output(['git', 'ls-files', '-co', '--exclude-standard'], cwd=root, text=True).splitlines())
paths = {path for path in paths if path in tracked or not path.startswith('results/')
         or any('primary' in part for part in Path(path).parts) or path.endswith('/cases.json')}
inputs = {path: sha(root / path) for path in sorted(paths) if (root / path).is_file()}
state = {
    'recorder_pid': os.getpid(), 'scope': os.environ['SUSS_DURABLE_SCOPE'],
    'head': head, 'inputs': inputs, 'runner_sha256': sha(Path(__file__)),
    'phase': 'waiting-for-actual-numeric-integrated-terminal', 'complete': False,
    'commands': [], 'qualification': 'Diagnostic working overlay for review R3/R4 only. R1/R2 remain unresolved; no issue acceptance, Ready, full-workspace or final-head CI claim. Heavy gates wait for actual current numeric child terminal; no observation timeout interruption.'
}
assert state['scope'] in Path('/proc/self/cgroup').read_text()

def save():
    (out / 'commands.json').write_text(json.dumps(state, indent=2) + '\n')
    (base / 'self-values-false-metadata-validation-state.json').write_text(json.dumps({k: v for k, v in state.items() if k != 'inputs'}, indent=2) + '\n')

def unchanged():
    return (subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip() == head
            and all((root / path).is_file() and sha(root / path) == digest for path, digest in inputs.items())
            and sha(Path(__file__)) == state['runner_sha256'])

save()
try:
    while True:
        previous = json.loads(numeric.read_text())
        assert previous['head'] == '9bc150994593a682acfb3a861e87bf9942881e23'
        assert previous['recorder_pid'] == 2733805
        if previous.get('complete'):
            assert previous['inputs_unchanged'] and previous['head_unchanged']
            assert all(row.get('complete') for row in previous['commands'])
            # A receipt can be written just before final recorder teardown.
            proc = Path('/proc/2733805/status')
            if not proc.exists() or 'State:\tZ' in proc.read_text():
                break
        time.sleep(10)
    state['numeric_terminal'] = {
        'receipt': str(numeric), 'sha256': sha(numeric),
        'commands': previous['commands'], 'complete': True,
        'all13_passed': len(previous['commands']) == 13 and all(row.get('exit_code') == 0 for row in previous['commands'])
    }
    # Retire only the held future-stage controller, after its actual child has
    # finished. No Cargo, runner, process group or active validation is stopped.
    parent = Path('/proc/2403439')
    retirement = {'reason': 'Required shared parent review fixes make later numeric full validation premature.', 'active_validation_interrupted': False}
    if parent.exists():
        assert b'continue-numeric-after-fa0da-workspace.py' in (parent / 'cmdline').read_bytes()
        assert 'State:\tT' in (parent / 'status').read_text()
        assert 'suss-m12-numeric-fa0da-workspace.scope' in (parent / 'cgroup').read_text()
        os.kill(2403439, signal.SIGKILL)
        retirement.update(pid=2403439, signal='SIGKILL', controller_plan_complete=False, later_full_started=False)
    else:
        retirement['controller_already_absent'] = True
    (base / 'numeric-fa0da-future-stages-controller-retirement.json').write_text(json.dumps(retirement, indent=2) + '\n')
    assert unchanged(), 'Queued J overlay changed before gate execution; refuse to relabel it.'
    target = root.parent / 'm12-some-fn-target'
    binaryen = root.parent / 'm11-binaryen/binaryen-version_133/bin'
    env = os.environ | {'CARGO_TARGET_DIR': str(target), 'CARGO_BUILD_JOBS': '1', 'RUSTUP_TOOLCHAIN': 'stable', 'BINARYEN_CORES': '1', 'SUSS_WASM_OPT': str(binaryen / 'wasm-opt')}
    env['PATH'] = str(binaryen) + os.pathsep + env['PATH']
    state['binaryen'] = {str(binaryen / name): sha(binaryen / name) for name in ['wasm-opt', 'wasm-merge']}
    state['phase'] = 'executing-working-overlay-diagnostics'
    save()
    for name, command in [
        ('core-import', ['python3', 'scripts/core_import.py', '--check']),
        ('new-self-values-false-metadata-controls', ['cargo', 'test', '-p', 'suss-cli', '--locked', '--test', 'clojure_eval', 'false_macro_metadata', '--', '--nocapture', '--test-threads=2']),
        ('complete-runtime-abi', ['cargo', 'test', '--workspace', '--locked', '--test', 'runtime_abi', '--', '--nocapture', '--test-threads=2']),
    ]:
        assert unchanged(), 'Overlay changed between diagnostic commands.'
        row = {'name': name, 'command': command, 'complete': False}
        state['commands'].append(row)
        save()
        start = time.time()
        with (out / (name + '.log')).open('w') as log:
            process = subprocess.run(command, cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT)
        row.update(complete=True, exit_code=process.returncode, seconds=time.time() - start)
        save()
        print(row, flush=True)
        if process.returncode:
            break
    state.update(complete=True, phase='actual-working-overlay-terminal', inputs_unchanged=unchanged(),
                 binaryen_unchanged=all(sha(Path(path)) == digest for path, digest in state['binaryen'].items()))
    state['exit_code'] = next((row['exit_code'] for row in state['commands'] if row.get('exit_code')), 0)
    if not state['inputs_unchanged'] or not state['binaryen_unchanged']:
        state['exit_code'] = state['exit_code'] or 2
    save()
except Exception as error:
    state.update(complete=True, phase='controller-refusal-or-error', exit_code=2, error=repr(error))
    save()
    raise
raise SystemExit(state['exit_code'])
