"""Execute production entropy, selected WIT AOT and both public numeric APIs."""
from pathlib import Path
import hashlib,json,os,subprocess,sys,time,signal
ROOT=Path(__file__).resolve().parents[2];BASE=Path(__file__).resolve().parent
signal.signal(signal.SIGHUP, signal.SIG_IGN)
scope=os.environ.get("SUSS_DURABLE_SCOPE")
assert scope and scope in Path("/proc/self/cgroup").read_text(), "Launch the recorder itself inside the named bounded systemd scope"
OUT=BASE/sys.argv[1];OUT.mkdir(exist_ok=True);assert not (OUT/"commands.json").exists()
assert os.environ.get("SUSS_RANDOM_TARGET"),"Select the isolated owned target"
paths=subprocess.check_output(["git","ls-files","--cached","--others","--exclude-standard","-z"],cwd=ROOT).decode().split("\0")
sha=lambda raw:hashlib.sha256(raw).hexdigest()
inputs={p:sha((ROOT/p).read_bytes()) for p in paths if p and not p.startswith("results/") and (ROOT/p).is_file()}
for name in ["controlled-transformations.json","command.json","oracle.clj","upstream-rand.clj","upstream-rand-int.clj","rand-adapted.sus","rand-int-adapted.sus","rand.patch","rand-int.patch","public-effects/cases.json","public-effects/command.json","public-effects/oracle.clj"]:
 p=BASE/name;inputs[str(p.relative_to(ROOT))]=sha(p.read_bytes())
env=os.environ|{"CARGO_TARGET_DIR":os.environ["SUSS_RANDOM_TARGET"],"CARGO_BUILD_JOBS":"1","RUSTUP_TOOLCHAIN":"stable","XDG_RUNTIME_DIR":"/run/user/1000"}
tool=Path(os.environ["SUSS_WASM_OPT"]).resolve()
tool_record=json.loads((BASE/"binaryen-tool.json").read_text())
assert all(sha((tool.parent/name).read_bytes())==h for name,h in tool_record["verified_tool_sha256"].items())
record={"head":subprocess.check_output(["git","rev-parse","HEAD"],cwd=ROOT,text=True).strip(),"inputs":inputs,"TMPDIR":env.get("TMPDIR"),"runner_sha256":sha(Path(__file__).read_bytes()),"commands":[],"scope":scope,"recorder_pid":os.getpid(),"binaryen":{"wasm_opt":str(tool),"sha256":tool_record["verified_tool_sha256"]},"complete":False}
def save():(OUT/"commands.json").write_text(json.dumps(record,indent=2)+"\n")
commands=[
 ("runtime-entropy",["cargo","test","-p","suss-compile","--locked","--test","runtime_random","--","--nocapture","--test-threads=2"]),
 ("selected-wit-aot",["cargo","test","-p","suss-compile","--locked","--test","portable_random","--","--include-ignored","--nocapture","--test-threads=2"]),
 ("public-core-both-phases",["cargo","test","-p","suss-compile","--locked","--lib","random_tests","--","--nocapture","--test-threads=2"])]
save();exit_code=0
for label,command in commands:
 start=time.time()
 with (OUT/(label+".log")).open("w") as log:
  result=subprocess.run(command,cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT)
 entry={"label":label,"command":command,"exit_code":result.returncode,"seconds":time.time()-start}
 record["commands"].append(entry);save();print(entry,flush=True)
 if result.returncode:
  exit_code=result.returncode;break
record.update(binaryen_unchanged=all(sha((tool.parent/name).read_bytes())==h for name,h in tool_record["verified_tool_sha256"].items()),complete=True,exit_code=exit_code,inputs_unchanged=all(sha((ROOT/p).read_bytes())==h for p,h in inputs.items()));save()
print({k:v for k,v in record.items() if k not in ["inputs","commands"]},flush=True)
raise SystemExit(exit_code)
