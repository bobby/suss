from pathlib import Path
import os,subprocess,json,time,hashlib
root=Path.cwd();outer=root.parents[2];p=root/'results/m12-letfn';source=outer/'target/probe/m12-letfn-boxed-source'
r=json.loads((p/'boxed-inputs.json').read_text())
for name,sha in r['files'].items():assert hashlib.sha256((source/name).read_bytes()).hexdigest()==sha
cmd=['cargo','+stable','test','--workspace','--locked','--test','clojure_letfn','--','--test-threads=2']
env=os.environ|{'RUSTUP_TOOLCHAIN':'stable','CARGO_BUILD_JOBS':'2','CARGO_TARGET_DIR':str(outer/'target/probe/m12-letfn-boxed-target'),'SUSS_BUILD_DIR':str(outer/'target/suss'),'TMPDIR':str(source/'target/probe/tmp'),'XDG_RUNTIME_DIR':'/run/user/1000'}
(source/'target/probe/tmp').mkdir(parents=True,exist_ok=True);start=time.time()
with (p/'boxed-focused.log').open('w') as log:result=subprocess.run(['systemd-run','--user','--scope','-p','MemoryMax=4G','--',*cmd],cwd=source,env=env,stdout=log,stderr=subprocess.STDOUT)
r={'command':cmd,'cwd':str(source),'source_inputs':'boxed-inputs.json','exit_code':result.returncode,'seconds':time.time()-start};p.joinpath('boxed-focused-command.json').write_text(json.dumps(r,indent=2)+'\n');print(result.returncode,flush=True);raise SystemExit(result.returncode)
