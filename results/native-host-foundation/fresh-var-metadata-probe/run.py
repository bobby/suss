from pathlib import Path
import gzip, hashlib, json, os, subprocess, time

OUT = Path(__file__).resolve().parent
C = OUT.parent.parent / 'native-host-var-foundation'
T = Path('/home/bobby/.codex-workspaces/worktrees/ios-cd7ccbc9-1c87-43fe-95e7-2b6b71168849/suss/target/probe/m12-some-fn-target')
LIB = T / 'debug/deps/libsuss_compile-d24d0e8fe0a40796.rlib'
BIN = Path('/home/bobby/.codex-workspaces/worktrees/ios-cd7ccbc9-1c87-43fe-95e7-2b6b71168849/suss/target/probe/m11-binaryen/binaryen-version_133/bin')
sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
files = [OUT / 'probe.rs', Path(__file__), LIB, T / 'debug/suss', C / 'crates/suss-compile/src/portable_macros.rs', C / 'crates/suss-compile/src/portable_session/evaluation/native_values/macro_vars.rs']
inputs = {str(path): sha(path) for path in files}
head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=C, text=True).strip()
record = {'head': head, 'inputs': inputs, 'commands': [], 'complete': False, 'qualification': 'Read-only source probe against the existing compiled library. Exit42 signals a detected leak, not conformance success. No Cargo or repository source edits.'}
def save():
    (OUT / 'commands.json').write_text(json.dumps(record, indent=2) + '\n')
env = os.environ | {'BINARYEN_CORES': '1', 'SUSS_WASM_OPT': str(BIN / 'wasm-opt')}
env['PATH'] = str(BIN) + os.pathsep + env['PATH']
save()
code = 0
for name, cmd in [('compile', ['rustup', 'run', 'stable', 'rustc', '--edition=2024', str(OUT / 'probe.rs'), '--extern', 'suss_compile=' + str(LIB), '-L', 'dependency=' + str(T / 'debug/deps'), '-o', str(OUT / 'probe')]), ('execute', [str(OUT / 'probe'), str(C)])]:
    row = {'name': name, 'command': cmd, 'complete': False}
    record['commands'].append(row)
    save()
    start = time.monotonic()
    with (OUT / (name + '.stdout')).open('w') as stdout, (OUT / (name + '.stderr')).open('w') as stderr:
        proc = subprocess.Popen(cmd, cwd=OUT, env=env, stdout=stdout, stderr=stderr)
        row['pid'] = proc.pid
        save()
        code = proc.wait()
    row.update(complete=True, exit_code=code, seconds=time.monotonic() - start)
    save()
    if code:
        break
record.update(complete=True, exit_code=code, inputs_unchanged=all(sha(Path(path)) == digest for path, digest in inputs.items()), head_unchanged=head == subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=C, text=True).strip())
save()
(OUT / 'runner.py.gz').write_bytes(gzip.compress(Path(__file__).read_bytes(), mtime=0))
print(json.dumps({key: value for key, value in record.items() if key != 'inputs'}, indent=2))
print((OUT / (name + '.stdout')).read_text())
print((OUT / (name + '.stderr')).read_text())
raise SystemExit(code)
