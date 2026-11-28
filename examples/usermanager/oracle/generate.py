#!/usr/bin/env python3
"""Execute the shared application on pinned JVM Clojure and retain its raw output."""
from pathlib import Path
import hashlib
import json
import subprocess

APP = Path(__file__).resolve().parents[1]
ROOT = APP.parents[1]
OUT = ROOT / 'results/m5-usermanager/jvm-primary'
OUT.mkdir(parents=True, exist_ok=True)
inputs = ['deps.edn', 'oracle/run.clj', 'oracle/usermanager/storage.clj',
          'src/usermanager/app.cljc', 'fixtures/scenario.json', 'oracle/generate.py']
def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()
before = {name: sha(APP / name) for name in inputs}
command = ['clojure', '-M', 'oracle/run.clj', 'fixtures/scenario.json']
result = subprocess.run(command, cwd=APP, capture_output=True)
(OUT / 'stdout.jsonl').write_bytes(result.stdout)
(OUT / 'stderr.log').write_bytes(result.stderr)
receipt = {'command': command, 'exit_code': result.returncode, 'inputs': before,
           'changed_inputs': [name for name in inputs if sha(APP / name) != before[name]],
           'stdout_sha256': hashlib.sha256(result.stdout).hexdigest(),
           'stderr_sha256': hashlib.sha256(result.stderr).hexdigest(),
           'clojure': '1.12.6', 'java_version': subprocess.run(['java','-version'],capture_output=True,text=True).stderr}
(OUT / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
if result.returncode: raise SystemExit(result.returncode)
assert not receipt['changed_inputs']
rows = [json.loads(line) for line in result.stdout.splitlines()]
assert len(rows) == len(json.loads((APP / 'fixtures/scenario.json').read_text())['operations'])
(APP / 'fixtures/jvm-observations.jsonl').write_bytes(result.stdout)
print(f'{len(rows)} shared application observations recorded')
