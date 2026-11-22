"""Run the unfiltered baseline on the recorded source, retaining its real status."""
from pathlib import Path
import subprocess,os,json,time
root=Path.cwd();p=root/'results/m12-escape';outer=root.parents[2]
subprocess.run(['python3',str(p/'source-check.py')],check=True)
env=os.environ|{'RUSTUP_TOOLCHAIN':'stable','CARGO_BUILD_JOBS':'2','CARGO_TARGET_DIR':str(outer/'target/probe/m12-hints-target'),'SUSS_BUILD_DIR':str(outer/'target/suss'),'XDG_RUNTIME_DIR':'/run/user/1000'}
binpath=outer/'target/probe/m11-binaryen/binaryen-version_133/bin';env['SUSS_WASM_OPT']=str(binpath/'wasm-opt');env['PATH']=str(binpath)+':'+env['PATH'];env['TMPDIR']=str(root/'target/probe/tmp')
cmd=['cargo','test','--workspace','--locked','--','--test-threads=2']
record={'command':cmd,'source_inputs':'source-inputs.json','source_commit':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'exit_code':None}
(p/'full-workspace-command.json').write_text(json.dumps(record,indent=2)+'\n');start=time.time()
with (p/'full-workspace.log').open('w') as log:r=subprocess.run(['systemd-run','--user','--scope','-p','MemoryMax=6G','--',*cmd],env=env,stdout=log,stderr=subprocess.STDOUT)
record['exit_code']=r.returncode;record['seconds']=time.time()-start;(p/'full-workspace-command.json').write_text(json.dumps(record,indent=2)+'\n');print('full workspace',r.returncode,flush=True);raise SystemExit(r.returncode)
