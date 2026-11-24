"""Link original body tests against the frozen some-fn producer, without Cargo mutation."""
from pathlib import Path
import hashlib,json,os,subprocess,time
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent/"before-original-bodies"
OUT.mkdir(exist_ok=True);assert not (OUT/"command.json").exists()
TARGET=ROOT.parent/"m12-some-fn-target";DEPS=TARGET/"debug/deps";FP=TARGET/"debug/.fingerprint"
sha=lambda raw:hashlib.sha256(raw).hexdigest()
library=DEPS/"libsuss_compile-2874475a5bbbffec.rlib";assert library.is_file()
dependencies=json.loads((FP/"suss-compile-2874475a5bbbffec/lib-suss_compile.json").read_text())
def exact(name):
    wanted=next(row[3] for row in dependencies["deps"] if row[1]==name)
    choices=[]
    for p in FP.glob("*/lib-"+name):
        try: value=int.from_bytes(bytes.fromhex(p.read_text()),"little")
        except ValueError:continue
        if value==wanted:
            filename=DEPS/("lib"+name+"-"+p.parent.name.rsplit("-",1)[1]+".rlib")
            if filename.is_file():choices.append(filename)
    assert len(choices)==1,(name,choices)
    return choices[0]
libs={"suss_compile":library,**{name:exact(name) for name in ["suss_reader","serde_json","wasmtime"]}}
# TempDir stays entirely in the test host; no production API consumes its type.
libs["tempfile"]=max(DEPS.glob("libtempfile-*.rlib"),key=lambda p:p.stat().st_mtime_ns)
artifact=ROOT.parent/"m12-assert-before-artifact";artifact.mkdir(exist_ok=True)
scratch=artifact/"temporary";scratch.mkdir(exist_ok=True)
exe=artifact/"original-bodies";assert not exe.exists()
command=["rustc","--edition=2024","--test",str(ROOT/"crates/suss-compile/tests/clojure_test_suite.rs"),"-L","dependency="+str(DEPS),"-o",str(exe)]
for name,path in libs.items():command.extend(["--extern",name+"="+str(path)])
paths=subprocess.check_output(["git","ls-files","-z"],cwd=ROOT).decode().split("\0")
inputs={p:sha((ROOT/p).read_bytes()) for p in paths if p and (ROOT/p).is_file()}
core="clojurescript/src/main/cljs/cljs/core.cljs";inputs[core]=sha((ROOT/core).read_bytes())
libraries={str(p):sha(p.read_bytes()) for p in libs.values()}
record={"head":subprocess.check_output(["git","rev-parse","HEAD"],cwd=ROOT,text=True).strip(),"qualification":"Original unchanged some-fn/constantly body targets linked against immutable ea924dec producer libraries from the live isolated some-fn target. No Cargo target is written and no assert is supplied locally. This is not a Cargo pass.","inputs":inputs,"libraries":libraries,"commands":[],"complete":False}
def save():(OUT/"command.json").write_text(json.dumps(record,indent=2)+"\n")
def run(label,cmd):
    start=time.time()
    with (OUT/(label+".stdout")).open("w") as stdout,(OUT/(label+".stderr")).open("w") as stderr:
        result=subprocess.run(["systemd-run","--user","--scope","-p","MemoryMax=3G","--",*cmd],
          cwd=ROOT,env=os.environ|{"CARGO_MANIFEST_DIR":str(ROOT/"crates/suss-compile"),"RUSTUP_TOOLCHAIN":"stable","XDG_RUNTIME_DIR":"/run/user/1000","TMPDIR":str(scratch),"SUSS_SOME_FN_BODY_CAPTURE":str(OUT/"capture")},
          stdout=stdout,stderr=stderr)
    record["commands"].append({"label":label,"command":cmd,"exit_code":result.returncode,"seconds":time.time()-start});save()
    print(record["commands"][-1],flush=True);return result.returncode
save()
if run("build",command)==0:
    record["executable_sha256"]=sha(exe.read_bytes());save()
    run("bodies",[str(exe),"pinned_some_fn_bodies","--nocapture","--test-threads=2"])
record.update(complete=True,inputs_unchanged=all(sha((ROOT/p).read_bytes())==h for p,h in inputs.items()),
              libraries_unchanged=all(sha(Path(p).read_bytes())==h for p,h in libraries.items()))
save();print("before original bodies complete",record["inputs_unchanged"],flush=True)
