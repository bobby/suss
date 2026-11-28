from pathlib import Path
import subprocess,hashlib,json,time
root=Path.cwd();p=root/'target/probe/m5-rust-guest'
compiler=root/'target/probe/m5-usermanager/target/probe/usermanager-final-bin/suss'
paths=[compiler,p/'api.wit',p/'app.sus',p/'Cargo.lock']
sha=lambda f:hashlib.sha256(f.read_bytes()).hexdigest()
command=[str(compiler),'--no-cache','compile',str(p/'app.sus'),'-w',str(p/'api.wit'),'--wit-world','suss-consumer','--export','echo=fixture.app/echo','--import','fixture:rust-interop/provider@0.1.0#echo=fixture.host/echo','-o',str(p/'consumer.wasm')]
r={'command':command,'inputs':{str(f):sha(f) for f in paths},'qualification':'Prototype uses immutable application compiler snapshot 2a779949; no source workspace was edited.'}
start=time.monotonic()
with (p/'compile-suss.log').open('wb') as log: result=subprocess.run(command,stdout=log,stderr=subprocess.STDOUT)
r.update(exit_code=result.returncode,seconds=time.monotonic()-start,changed_inputs=[str(f) for f in paths if sha(f)!=r['inputs'][str(f)]])
if result.returncode==0:r['component_sha256']=sha(p/'consumer.wasm')
(p/'compile-suss.json').write_text(json.dumps(r,indent=2)+'\n');print(json.dumps(r));raise SystemExit(result.returncode)
