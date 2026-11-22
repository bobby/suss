from pathlib import Path
import hashlib,json,os,subprocess,time
root=Path(__file__).resolve().parents[2];out=Path(__file__).resolve().parent
command=['cargo','test','-p','suss-compile','--locked','--test','portable_value_decoder','--test','clojure_test_suite','--','--nocapture','--test-threads=2']
env=os.environ|{'CARGO_BUILD_JOBS':'2','RUSTUP_TOOLCHAIN':'stable','XDG_RUNTIME_DIR':'/run/user/1000','CARGO_TARGET_DIR':str(root.parents[2]/'target/probe/m12-integer-target'),'SUSS_CLOJURE_TEST_SUITE_CAPTURE':str(out)}
record={'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),'command':command,'complete':False,'changed_inputs':{p:hashlib.sha256((root/p).read_bytes()).hexdigest() for p in subprocess.check_output(['git','diff','--name-only'],cwd=root,text=True).splitlines()}};(out/'command.json').write_text(json.dumps(record,indent=2)+'\n')
started=time.time()
with (out/'run.log').open('w') as log:r=subprocess.run(['systemd-run','--user','--scope','-p','MemoryMax=4G','--',*command],cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
record|={'exit_code':r.returncode,'seconds':time.time()-started,'complete':True};(out/'command.json').write_text(json.dumps(record,indent=2)+'\n');print(record,flush=True)
