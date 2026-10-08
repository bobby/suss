#!/usr/bin/env python3
# Original isolated development oracle; repository MIT/Apache-2.0 terms.
# Compare fresh pinned CLJS observations with frozen expectations. Never rewrite them.
import argparse, json, subprocess
from pathlib import Path
P=Path(__file__).parent
PIN='c4295f303100bbf5afac449242d30bca1126f1a1'
a=argparse.ArgumentParser();a.add_argument('observations',type=Path);a.add_argument('--native',type=Path,help='existing bridge binary; executes artifacts, no Cargo');args=a.parse_args()
observed=json.loads(args.observations.read_text());expected=json.loads((P/'expected.json').read_text())
assert set(observed)=={'upstream','node','cases'} and observed['upstream']==PIN and observed['node']=='v24.5.0'
assert observed['cases']==expected, (observed['cases'],expected)
r=subprocess.run(['/opt/homebrew/bin/node',str(P/'check-oracle.cjs')],capture_output=True,text=True,timeout=15);assert r.returncode==0,r.stdout+r.stderr
print('PASS: fresh pinned CLJS 7 source examples exactly match frozen result/journal/count bits and exported graph model')
if args.native:
 for name,exp in expected.items():
  r=subprocess.run([str(args.native),'--effects',str(P/(name+'.json'))],capture_output=True,text=True,timeout=30)
  assert r.returncode==0,(name,r.stdout,r.stderr)
  native=json.loads(r.stdout)
  assert set(native)=={'schema','abi_initializer_gate','result_bits','cells'} and native['schema']=='suss.mlir.effects.observation.v1' and native['abi_initializer_gate'] is True
  assert len(native['cells'])==2 and all(set(c)=={'namespace','name','bits'} for c in native['cells'])
  # Native CLI exact output shape is coordinator-owned; fail rather than infer.
  assert native['result_bits']==exp['result_bits'],(name,native,exp)
  cells={(c['namespace'],c['name']):c['bits'] for c in native['cells']}
  assert cells=={('effects','journal'):exp['journal_bits'],('effects','count'):exp['count_bits']},(name,cells,exp)
 print('PASS: actual native 7 effects artifacts agree with fresh pinned CLJS observations')
else:print('Native execution not requested; pending coordinator gate')
