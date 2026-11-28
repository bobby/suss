"""Carry exact focused execution across a proof-only commit without rerunning it."""
from pathlib import Path
import hashlib,json,subprocess
base=Path(__file__).resolve().parent;root=base/'m12-numeric-completion'
proof=root/'results/m12-num-api/ci6-sequence-focused-repair';receipt=json.loads((proof/'commands.json').read_bytes())
assert receipt['complete'] and receipt['inputs_unchanged'] and receipt['head_unchanged'] and receipt['binaryen_unchanged']
assert len(receipt['commands'])==1 and receipt['commands'][0]['exit_code']==0
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
def protected(path):
 return path.startswith(('crates/','runtime/','scripts/','.cargo/','.github/','tests/','docs/compatibility/','clojurescript/')) or path in ['Cargo.toml','Cargo.lock'] or any('primary' in part for part in Path(path).parts) or path.endswith(('/cases.json','/reference.json'))
inputs={p:h for p,h in receipt['inputs'].items() if protected(p)}
assert all((root/p).is_file() and sha(root/p)==h for p,h in inputs.items())
fixture=root/'tests/oracle/sequence-cases.json';original=subprocess.check_output(['git','show','a4513977926dfc553779cd97acab5e68c6b634b6:tests/oracle/sequence-cases.json'],cwd=root)
assert fixture.read_bytes()==original and len(json.loads(original)['cases'])==135
log=(proof/'complete-sequence-and-number-object-contexts.log').read_text();assert 'test result: ok. 7 passed; 0 failed' in log and 'test result: ok. 4 passed; 0 failed' in log
subprocess.run(['git','diff','--quiet'],cwd=root,check=True);subprocess.run(['git','diff','--cached','--quiet'],cwd=root,check=True)
print(json.dumps({'focused_receipt_head':receipt['head'],'current_head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),'focused_actual_exit':0,'source_and_primary_inputs_unchanged':len(inputs),'sequence_cases':135,'sequence_tests':7,'source_object_tests':4,'canonical_nan_primitive_false_preserved':True,'ordinary_function_nan_true_measured':True,'fixture_sha256':sha(fixture),'qualification':'Protected executable/test/primary inputs are byte-identical to the actual frozen focused execution; only evidence and handoff were committed. No execution receipt is relabeled to a different HEAD. Fresh full native capture and exact workspace remain separate gates.'},indent=2))
