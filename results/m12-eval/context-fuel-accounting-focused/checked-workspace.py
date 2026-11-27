"""Propagate the exact Cargo result recorded by the existing workspace recorder."""
from pathlib import Path
import json,subprocess,sys

def checked(path,expected=None):
 r=json.loads(path.read_bytes())
 assert r['complete'] and isinstance(r['exit_code'],int), 'no actual complete Cargo terminal'
 assert r['inputs_unchanged'] and r['head_unchanged'], 'workspace source/HEAD changed during execution'
 assert r['command']==['cargo','test','--workspace','--locked','--','--test-threads=2']
 if expected is not None:assert r['head']==expected
 print(json.dumps({'workspace_receipt':str(path),'head':r['head'],'actual_cargo_exit_code':r['exit_code'],'complete':True}),flush=True)
 return r['exit_code']

if sys.argv[1]=='--receipt-only':
 # Historical terminal parsing only; this mode does not validate current source.
 raise SystemExit(checked(Path(sys.argv[2])))
out=Path(sys.argv[1]).resolve();expected=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
r=subprocess.run(['python3','results/m12-random/run-workspace.py',str(out)])
code=checked(out/'command.json',expected)
assert r.returncode in [0,code], 'recorder failed independently of recorded Cargo result'
raise SystemExit(code)
