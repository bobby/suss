"""Frozen native missing-binding probes; no local function or macro replacement."""
from pathlib import Path
import gzip, hashlib, json, os, shutil, subprocess, time
ROOT=Path(__file__).resolve().parents[2]
BASE=Path(__file__).resolve().parent
OUT=BASE/"before-shipped-some-fn"
OUT.mkdir(exist_ok=True)
assert not (OUT/"command.json").exists()
sha=lambda raw:hashlib.sha256(raw).hexdigest()
receipt=json.loads(gzip.decompress((BASE/"after-checked-comparison-capture/command.json.gz").read_bytes()))
assert receipt["head"]=="9d501d71028841854afcc8b1835767cc48d4c449"
assert receipt["complete"] and receipt["exit_code"]==0 and receipt["inputs_unchanged"]
differences=[]
for p,h in receipt["inputs"].items():
    current=sha((ROOT/p).read_bytes())
    if current!=h:
        assert p in ["docs/compatibility/README.md","docs/roadmap/handoff.md"],p
        differences.append({"path":p,"built_input":h,"current_input":current})
artifact=ROOT.parent/"m12-some-fn-before-artifact"
artifact.mkdir(exist_ok=True)
original=ROOT.parent/"m12-suite-platform-target/debug/suss"
exe=artifact/"suss-before"
assert not exe.exists()
shutil.copy2(original,exe)
digest=sha(exe.read_bytes())
assert digest==sha(original.read_bytes())
scratch=artifact/"temporary"
scratch.mkdir(exist_ok=True)
paths=subprocess.check_output(["git","ls-files","-z"],cwd=ROOT).decode().split("\0")
inputs={p:sha((ROOT/p).read_bytes()) for p in paths if p and (ROOT/p).is_file()}
p="clojurescript/src/main/cljs/cljs/core.cljs";inputs[p]=sha((ROOT/p).read_bytes())
record={"head":subprocess.check_output(["git","rev-parse","HEAD"],cwd=ROOT,text=True).strip(),
        "binary_sha256":digest,"binary_source_head":receipt["head"],
        "source_qualification":{"all_recorded_production_test_fixture_inputs_match":True,"only_documentation_differences":differences},
        "inputs":inputs,"commands":[],"complete":False,
        "qualification":"Native CLI copied immutably from the passing9d501d71 candidate. All its recorded inputs match this checkout except listed documentation. No some-fn/assert implementation is supplied locally, and no Cargo target is mutated."}
def save():(OUT/"command.json").write_text(json.dumps(record,indent=2)+"\n")
save()
for name,source in [("some-fn","(some-fn identity)"),
                    ("assert","(fn [] (assert false \"short-circuit probe\"))")]:
    command=[str(exe),"--no-cache","-e",source]
    started=time.time()
    with (OUT/(name+".stdout")).open("w") as stdout,(OUT/(name+".stderr")).open("w") as stderr:
        result=subprocess.run(["systemd-run","--user","--scope","-p","MemoryMax=3G","--",*command],
            cwd=ROOT,env=os.environ|{"XDG_RUNTIME_DIR":"/run/user/1000","TMPDIR":str(scratch)},
            stdout=stdout,stderr=stderr)
    record["commands"].append({"name":name,"command":command,"exit_code":result.returncode,"seconds":time.time()-started})
    save()
    print(record["commands"][-1],flush=True)
record.update(complete=True,binary_unchanged=sha(exe.read_bytes())==digest,
              inputs_unchanged=all(sha((ROOT/p).read_bytes())==h for p,h in inputs.items()))
save()
print({k:v for k,v in record.items() if k not in ["inputs","commands"]},flush=True)
