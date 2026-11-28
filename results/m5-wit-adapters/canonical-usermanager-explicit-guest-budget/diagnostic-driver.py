#!/usr/bin/env python3
"""Diagnostic request costs after an isolated allocator replacement, not a benchmark."""
from pathlib import Path
import hashlib,json,subprocess
paths=[Path('target/probe/usermanager-budget-diagnostic'),Path('target/probe/usermanager-optimized-private-index.wasm')]
before={str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in paths}
print(json.dumps({'artifact_identities':before,'qualification':'Diagnostic only: list exports receive3-billion fuel, production defaults stay100-million; only the canonical allocator core module was replaced in the test artifact.'}),flush=True)
for size in [1000,10000]:
 command=[str(paths[0]),str(paths[1]),str(size)]
 print(json.dumps({'command':command}),flush=True)
 result=subprocess.run(command)
 print(json.dumps({'dataset':size,'exit_code':result.returncode}),flush=True)
 if result.returncode:raise SystemExit(result.returncode)
assert all(hashlib.sha256(p.read_bytes()).hexdigest()==before[str(p)] for p in paths)
