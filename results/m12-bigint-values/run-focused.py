from pathlib import Path
import hashlib,json,os,subprocess,time
root=Path(__file__).resolve().parents[2];outer=root.parents[2];out=Path(__file__).resolve().parent
env=os.environ|{'CARGO_TARGET_DIR':str(outer/'target/probe/m12-bigint-target'),'CARGO_BUILD_JOBS':'2','RUSTUP_TOOLCHAIN':'stable','XDG_RUNTIME_DIR':'/run/user/1000'}
commands=[
 ('focused-final',['cargo','test','-p','suss-cli','--locked','--test','clojure_bigints','--','--test-threads=2']),
 ('numeric-controls',['cargo','test','-p','suss-cli','--locked','--test','clojure_longs_doubles','--test','clojure_equality_bitwise','--test','clojure_unboxed_locals','--','--test-threads=2']),
 ('abi-controls',['cargo','test','-p','suss-compile','--locked','--test','runtime_abi','--test','portable_calling_convention','--test','portable_comparator_foundations','--','--test-threads=2']),
]
files=subprocess.check_output(['git','diff','--name-only'],cwd=root,text=True).splitlines()+['crates/suss-compile/src/runtime_abi/bigints.rs','crates/suss-cli/tests/clojure_bigints.rs','results/m12-bigint-values/cases.json']
snapshot={f:hashlib.sha256((root/f).read_bytes()).hexdigest() for f in files if (root/f).is_file()}
record={'inputs':snapshot,'commands':[],'complete':False}
def save(): (out/'commands.json').write_text(json.dumps(record,indent=2)+'\n')
save()
for label,command in commands:
 start=time.time()
 with (out/(label+'.log')).open('w') as log:
  r=subprocess.run(['systemd-run','--user','--scope','-p','MemoryMax=4G','--',*command],cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
 record['commands'].append({'label':label,'command':command,'exit_code':r.returncode,'seconds':time.time()-start});save()
 print(label,r.returncode,flush=True)
 if r.returncode: raise SystemExit(r.returncode)
assert all(hashlib.sha256((root/f).read_bytes()).hexdigest()==h for f,h in snapshot.items())
record['complete']=True;save()
