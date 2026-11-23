"""Execute compiled assert flags, primary data and both complete original bodies."""
from pathlib import Path
import hashlib,json,os,subprocess,sys,time
ROOT=Path(__file__).resolve().parents[2];BASE=Path(__file__).resolve().parent
OUT=BASE/sys.argv[1];OUT.mkdir(exist_ok=True);assert not (OUT/"commands.json").exists()
assert os.environ.get("SUSS_ASSERT_TARGET"),"Select the isolated owned target"
paths=subprocess.check_output(["git","ls-files","--cached","--others","--exclude-standard","-z"],cwd=ROOT).decode().split("\0")
sha=lambda raw:hashlib.sha256(raw).hexdigest()
inputs={p:sha((ROOT/p).read_bytes()) for p in paths if p and not p.startswith("results/") and (ROOT/p).is_file()}
for name in ["cases.json","command.json","oracle.clj","upstream-assert.clj","assert-adapted.sus","assert.patch"]:
 p=BASE/name;inputs[str(p.relative_to(ROOT))]=sha(p.read_bytes())
env=os.environ|{"CARGO_TARGET_DIR":os.environ["SUSS_ASSERT_TARGET"],"CARGO_BUILD_JOBS":"1","RUSTUP_TOOLCHAIN":"stable","XDG_RUNTIME_DIR":"/run/user/1000","SUSS_SOME_FN_BODY_CAPTURE":str(OUT/"body-capture")}
record={"head":subprocess.check_output(["git","rev-parse","HEAD"],cwd=ROOT,text=True).strip(),"inputs":inputs,"TMPDIR":env.get("TMPDIR"),"runner_sha256":sha(Path(__file__).read_bytes()),"commands":[],"complete":False}
def save():(OUT/"commands.json").write_text(json.dumps(record,indent=2)+"\n")
commands=[
 ("compile-time-flags",["cargo","test","-p","suss-compile","--locked","--lib","assert_compilation_tests","--","--nocapture","--test-threads=2"]),
 ("runtime-macro-actual-aot",["cargo","test","--workspace","--locked","--test","clojure_assert","--","--test-threads=2"]),
 ("original-some-fn-and-constantly-bodies",["cargo","test","--workspace","--locked","--test","clojure_test_suite","pinned_some_fn_bodies","--","--nocapture","--test-threads=2"])]
save();exit_code=0
for label,command in commands:
 start=time.time()
 with (OUT/(label+".log")).open("w") as log:
  result=subprocess.run(["systemd-run","--user","--scope","-p","MemoryMax=3G","--",*command],cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT)
 entry={"label":label,"command":command,"exit_code":result.returncode,"seconds":time.time()-start}
 record["commands"].append(entry);save();print(entry,flush=True)
 if result.returncode:
  exit_code=result.returncode;break
record.update(complete=True,exit_code=exit_code,inputs_unchanged=all(sha((ROOT/p).read_bytes())==h for p,h in inputs.items()));save()
print({k:v for k,v in record.items() if k not in ["inputs","commands"]},flush=True)
raise SystemExit(exit_code)
