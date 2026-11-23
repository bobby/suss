"""Narrow ABI/capability feasibility, separate from any public rand acceptance."""
from pathlib import Path
import hashlib,json,os,subprocess,time
ROOT=Path(__file__).resolve().parents[3];BASE=Path(__file__).resolve().parent
assert not (BASE/"command.json").exists()
TARGET=ROOT.parent/"m12-some-fn-target";DEPS=TARGET/"debug/deps";FP=TARGET/"debug/.fingerprint"
sha=lambda raw:hashlib.sha256(raw).hexdigest()
cli=json.loads(next(FP.glob("suss-cli-*/bin-suss.json")).read_text())
compiler=json.loads((FP/"suss-compile-2874475a5bbbffec/lib-suss_compile.json").read_text())
def exact(parent,name):
    wanted=next(row[3] for row in parent["deps"] if row[1]==name);matches=[]
    for f in FP.glob("*/lib-"+name):
        try:fingerprint=int.from_bytes(bytes.fromhex(f.read_text()),"little")
        except ValueError:continue
        library=DEPS/("lib"+name+"-"+f.parent.name.rsplit("-",1)[1]+".rlib")
        if fingerprint==wanted and library.is_file():matches.append((library,json.loads(f.with_name(f.name+".json").read_text())))
    assert len(matches)==1,(name,matches)
    return matches[0]
wasmtime,wasmtime_meta=exact(cli,"wasmtime")
libs={"wasmtime":wasmtime,"wasmtime_wasi":exact(cli,"wasmtime_wasi")[0],
      "wasm_encoder":exact(compiler,"wasm_encoder")[0],"wat":exact(wasmtime_meta,"wat")[0]}
artifact=ROOT.parent/"m12-random-provider-probe";artifact.mkdir(exist_ok=True)
exe=artifact/"probe";assert not exe.exists()
command=["rustc","--edition=2024",str(BASE/"main.rs"),"-L","dependency="+str(DEPS),"-o",str(exe)]
for name,path in libs.items():command.extend(["--extern",name+"="+str(path)])
record={"source_sha256":sha((BASE/"main.rs").read_bytes()),"libraries":{str(p):sha(p.read_bytes()) for p in libs.values()},"commands":[],"complete":False,"qualification":"Standalone feasibility fixture only: typed nullable callback installation, scalar conversion, explicit standard wasi:random/random@0.3.1 import and actual provider calls. It neither modifies the production runtime nor establishes public rand/error/size/ABI compatibility."}
def save():(BASE/"command.json").write_text(json.dumps(record,indent=2)+"\n")
save()
for name,cmd in [("build",command),("run",[str(exe)])]:
 start=time.time()
 with (BASE/(name+".stdout")).open("w") as stdout,(BASE/(name+".stderr")).open("w") as stderr:
  p=subprocess.run(["systemd-run","--user","--scope","-p","MemoryMax=3G","--",*cmd],cwd=ROOT,env=os.environ|{"XDG_RUNTIME_DIR":"/run/user/1000","RUSTUP_TOOLCHAIN":"stable"},stdout=stdout,stderr=stderr)
 record["commands"].append({"label":name,"command":cmd,"exit_code":p.returncode,"seconds":time.time()-start});save();print(record["commands"][-1],flush=True)
 if p.returncode:break
record.update(complete=True,libraries_unchanged=all(sha(Path(p).read_bytes())==h for p,h in record["libraries"].items()),source_unchanged=sha((BASE/"main.rs").read_bytes())==record["source_sha256"]);save()
raise SystemExit(record["commands"][-1]["exit_code"])
