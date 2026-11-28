from pathlib import Path
import hashlib,json,subprocess
root=Path.cwd();deps=root/'target/probe/cargo/debug/deps'
source=root/'target/probe/usermanager-large-canonical-observer.rs'
binary=root/'target/probe/usermanager-large-canonical-observer'
component=root/'target/probe/usermanager-measurements-final/application-0.wasm'
libs={'suss_usermanager_host':deps/'libsuss_usermanager_host-b530250e12750890.rlib','wasm_encoder':deps/'libwasm_encoder-0ca795981cba27c6.rlib','wasmparser':deps/'libwasmparser-d54bd7dcfed2f0c3.rlib'}
inputs=[source,root/'crates/suss-cli/tests/support/usermanager_canonical_stats.rs',component,root/'Cargo.lock',*libs.values()]
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
record={'qualification':'Standalone Rust harness using the identified prebuilt host library and unmodified normal CLI component, with test-only exact-prefix read-only canonical observer; host seeding isolates large lists. Not a Cargo/workspace baseline or timing comparison.','inputs':{str(p):sha(p) for p in inputs}}
command=['rustc','--edition=2024','-C','opt-level=1',str(source),'-L','dependency='+str(deps),'-o',str(binary)]
for name,path in libs.items():command+=['--extern',name+'='+str(path)]
record['build_command']=command
out=root/'target/probe/usermanager-large-canonical-observer-receipt.json'
out.write_text(json.dumps(record,indent=2)+'\n')
result=subprocess.run(command)
record['build_exit_code']=result.returncode
if result.returncode==0:
 record['binary_sha256']=sha(binary)
 record['run_command']=[str(binary),str(component)]
 result=subprocess.run(record['run_command'])
record['exit_code']=result.returncode
record['changed_inputs']=[str(p) for p in inputs if sha(p)!=record['inputs'][str(p)]]
out.write_text(json.dumps(record,indent=2)+'\n')
raise SystemExit(result.returncode)
