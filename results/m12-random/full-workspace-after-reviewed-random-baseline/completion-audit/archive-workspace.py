"""Archive a terminal, unchanged random/some-fn workspace baseline."""
from pathlib import Path
import gzip, hashlib, json, shutil, subprocess, sys

root = Path(sys.argv[1]).resolve()
target = Path(sys.argv[2]).resolve()
run = root / 'results/m12-random/full-workspace-after-reviewed-random-baseline'
receipt = json.loads((run / 'command.json').read_text())
assert receipt['complete'], 'original command remains live/incomplete'
assert receipt['exit_code'] == 0, receipt['exit_code']
assert receipt['inputs_unchanged'] and receipt['head_unchanged']
assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip() == receipt['head']
sha = lambda raw: hashlib.sha256(raw).hexdigest()
for path, expected in receipt['inputs'].items():
    assert sha((root / path).read_bytes()) == expected, path
out = run / 'completion-audit'
assert not out.exists(), 'preserve existing audit instead of overwriting'
out.mkdir()
manifest = {}

def archive(name, raw):
    compressed = gzip.compress(raw, mtime=0)
    (out / (name + '.gz')).write_bytes(compressed)
    manifest[name + '.gz'] = {'raw_sha256': sha(raw), 'archive_sha256': sha(compressed)}

archive('command.json', (run / 'command.json').read_bytes())
archive('run.log', (run / 'run.log').read_bytes())
archive('cargo-metadata.json', subprocess.check_output(
    ['cargo', 'metadata', '--locked', '--no-deps', '--format-version', '1'], cwd=root))
fingerprints = {}
for path in sorted((target / 'debug/.fingerprint').glob('*/test-*.json')):
    raw = path.read_bytes()
    fingerprints[str(path)] = {'content': raw.decode(), 'sha256': sha(raw)}
assert fingerprints
archive('cargo-fingerprints.json', json.dumps(fingerprints, indent=2).encode())
(out / 'archive-manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
shutil.copyfile(Path(__file__).with_name('replay-targets.py'), out / 'replay-targets.py')
report = subprocess.check_output([sys.executable, str(out / 'replay-targets.py')], cwd=root)
(out / 'replayed-target-coverage.json').write_bytes(report)
print(report.decode())
