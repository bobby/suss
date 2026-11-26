from pathlib import Path
import gzip,hashlib,json,subprocess
root=Path(__file__).resolve().parents[2];base=Path(__file__).resolve().parent
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
proof=json.loads((base/'self-values-metadata-checkpoint.json').read_text())
read=lambda name:json.loads(gzip.decompress((base/proof[name]).read_bytes()))
broad=read('broad_receipt');abi=read('abi_receipt');negative=read('negative_receipt')
assert broad['head']==abi['head']==negative['head']==proof['summary']['tested_head']
assert negative['complete'] and negative['exit_code']==101 and negative['inputs_unchanged']
for receipt in [broad,abi]:
 assert receipt['complete'] and receipt['exit_code']==0 and receipt['inputs_unchanged'] and receipt['binaryen_unchanged']
 assert all(row['complete'] and row['exit_code']==0 for row in receipt['commands'])
for path,digest in broad['inputs'].items():
 if path not in proof['prose_exemptions']:assert sha(root/path)==digest,path
for path,digest in abi['inputs'].items():
 if path not in proof['abi_only_unexecuted_exemptions']:assert sha(root/path)==digest,path
tracked=set(subprocess.check_output(['git','ls-files'],cwd=root,text=True).splitlines())
assert tracked-set(broad['inputs'])<=set(proof['publication_paths'])|set(proof['gitlinks'])
for path,oid in proof['gitlinks'].items():
 assert subprocess.check_output(['git','rev-parse','HEAD:'+path],cwd=root,text=True).strip()==oid
 assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=root/path,text=True).strip()==oid
for path in proof['publication_paths']:assert (root/path).is_file(),path
print('Actual frozen public62/host119 and production-identical ABI62 conserved; initial101 retained. Macro transport/state, new full workspace, review and final-head CI remain required.')
