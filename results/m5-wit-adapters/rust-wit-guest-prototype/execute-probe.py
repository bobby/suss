from pathlib import Path
import json,hashlib,time,subprocess
root=Path.cwd();p=root/'target/probe/m5-rust-guest';deps=root/'target/probe/m5-usermanager/target/probe/cargo/debug/deps'
files=[p/'execute',p/'execute.rs',p/'provider.wasm',p/'consumer.wasm',p/'api.wit',p/'Cargo.lock',p/'src/lib.rs',deps/'libwasmtime-7fad23f82b6a71a9.rlib',deps/'libwasm_encoder-0ca795981cba27c6.rlib',deps/'libwasmparser-d54bd7dcfed2f0c3.rlib']
sha=lambda f:hashlib.sha256(f.read_bytes()).hexdigest()
r={'qualification':'Prototype standalone actual Rust-host -> Suss -> pinned generated Rust-guest component. Explicit prebuilt native libraries; not a Cargo/workspace baseline.','inputs':{str(f):sha(f) for f in files},'command':[str(p/'execute'),str(p)]}
(p/'executed-source.rs').write_bytes((p/'execute.rs').read_bytes());start=time.monotonic()
with (p/'execute.log').open('wb') as log:result=subprocess.run(r['command'],stdout=log,stderr=subprocess.STDOUT)
r.update(exit_code=result.returncode,seconds=time.monotonic()-start,changed_inputs=[str(f) for f in files if sha(f)!=r['inputs'][str(f)]])
if (p/'composed.wasm').exists():r['composed_sha256']=sha(p/'composed.wasm')
(p/'execute.json').write_text(json.dumps(r,indent=2)+'\n');print(json.dumps(r));raise SystemExit(result.returncode)
