from pathlib import Path
import hashlib, json, os, subprocess, time
root=Path.cwd(); outer=root.parents[2]; dest=root/'results/m12-numeric-captures'
env=os.environ|{'RUSTUP_TOOLCHAIN':'stable','CARGO_BUILD_JOBS':'2','CARGO_TARGET_DIR':str(outer/'target/probe/m12-letfn-target'),'SUSS_BUILD_DIR':str(outer/'target/suss'),'XDG_RUNTIME_DIR':'/run/user/1000','TMPDIR':str(root/'target/probe/tmp')}
command=['cargo','+stable','test','--workspace','--locked','--test','portable_numeric_captures','--test','clojure_numeric_captures','--test','captured_fn_calls','--test','portable_closures','--test','portable_primitive_entries','--','--test-threads=2']
inputs=['crates/suss-compile/src/portable/emit.rs','crates/suss-compile/src/portable/emit/captures.rs','crates/suss-compile/tests/portable_numeric_captures.rs','crates/suss-compile/src/portable/emit/unboxed.rs','crates/suss-cli/tests/clojure_numeric_captures.rs']
record={'command':command,'source':{p:hashlib.sha256((root/p).read_bytes()).hexdigest() for p in inputs},'exit_code':None}
receipt=dest/'focused-only-command.json'; receipt.write_text(json.dumps(record,indent=2)+'\n')
start=time.time()
with (dest/'focused-only.log').open('w') as log:
 result=subprocess.run(['systemd-run','--user','--scope','-p','MemoryMax=4G','--',*command],env=env,stdout=log,stderr=subprocess.STDOUT)
record.update(exit_code=result.returncode,seconds=time.time()-start)
receipt.write_text(json.dumps(record,indent=2)+'\n')
print(record['exit_code'],flush=True)
raise SystemExit(result.returncode)
