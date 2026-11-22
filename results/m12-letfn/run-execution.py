from pathlib import Path
import os,json,subprocess,time
root=Path.cwd();outer=root.parents[2];p=root/'results/m12-letfn'
env=os.environ|{'RUSTUP_TOOLCHAIN':'stable','CARGO_BUILD_JOBS':'2','CARGO_TARGET_DIR':str(outer/'target/probe/m12-recursive-target'),'SUSS_BUILD_DIR':str(outer/'target/suss'),'TMPDIR':str(root/'target/probe/tmp'),'XDG_RUNTIME_DIR':'/run/user/1000'}
cmd=['cargo','+stable','test','--workspace','--locked','--test','clojure_letfn','--','--test-threads=2'];start=time.time()
with (p/'execution-28.log').open('w') as log:r=subprocess.run(['systemd-run','--user','--scope','-p','MemoryMax=4G','--',*cmd],env=env,stdout=log,stderr=subprocess.STDOUT)
(p/'execution-28-command.json').write_text(json.dumps({'command':cmd,'exit_code':r.returncode,'seconds':time.time()-start},indent=2)+'\n');print(r.returncode,flush=True);raise SystemExit(r.returncode)
