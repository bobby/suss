from pathlib import Path
import hashlib,json,os,subprocess,time
root=Path(__file__).resolve().parents[2];out=Path(__file__).resolve().parent
repository=root.parents[2];binary=repository/'target/probe/m12-bigint-target/debug/suss'
parent=repository/'target/probe/m12-bigint-source';scratch=repository/'target/probe/m12-ratio-before-84';scratch.mkdir(exist_ok=True)
artifact=scratch/'before.wasm';commands=[
 [str(binary),'compile',str(out/'before.sus'),'-w',str(out/'before.wit'),'-o',str(artifact),'--export','divide=ratios.before/divide','--export','literal=ratios.before/literal'],
 [str(binary),'run',str(artifact),'--invoke','divide','--','0'],
 [str(binary),'run',str(artifact),'--invoke','literal','--','0']]
record={'source_head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=parent,text=True).strip(),'executable':str(binary),'executable_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'source_sha256':hashlib.sha256((out/'before.sus').read_bytes()).hexdigest(),'wit_sha256':hashlib.sha256((out/'before.wit').read_bytes()).hexdigest(),'commands':[]}
for command in commands:
 start=time.time();r=subprocess.run(command,cwd=root,capture_output=True,text=True)
 record['commands'].append({'command':command,'exit_code':r.returncode,'seconds':time.time()-start,'stdout':r.stdout,'stderr':r.stderr})
 if len(record['commands'])==1:assert r.returncode==0,r.stderr
record['artifact_sha256']=hashlib.sha256(artifact.read_bytes()).hexdigest()
assert record['commands'][1]['exit_code']==0 and '0.3333333333333333' in record['commands'][1]['stdout']
assert record['commands'][2]['exit_code']!=0 and 'Unsupported numeric literal' in record['commands'][2]['stderr']
(out/'before-execution.json').write_text(json.dumps(record,indent=2)+'\n');print('Frozen pre-Ratio compiler returns a rounded double for / and rejects the Ratio literal, as expected.')
