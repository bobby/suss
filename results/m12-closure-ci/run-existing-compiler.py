"""Check the test-only repair without disturbing the live full Cargo run.

Use SUSS_EXISTING_TARGET to select the f5056ffe Cargo target directory.
Cargo CI performs the ordinary dependency-managed validation separately.
"""
from pathlib import Path
import hashlib
import json
import os
import subprocess

root = Path(__file__).resolve().parents[2]
out = root / "results/m12-closure-ci"
target = Path(os.environ["SUSS_EXISTING_TARGET"])
deps = target / "debug/deps"
fp = deps.parent / ".fingerprint"
lib = max(deps.glob("libsuss_compile-*.rlib"), key=lambda p: p.stat().st_mtime)
libhash = lib.stem.split("-")[1]
metadata = json.loads((fp / ("suss-compile-" + libhash) / "lib-suss_compile.json").read_text())
libs = {"suss_compile": lib}
for _, name, _, fingerprint in metadata["deps"]:
    if name not in ["wasmparser", "wasm_encoder"]:
        continue
    wanted = fingerprint.to_bytes(8, "little").hex()
    matches = [p for p in fp.glob(name.replace("_", "-") + "-*/lib-" + name) if p.read_text() == wanted]
    assert len(matches) == 1, (name, matches)
    libs[name] = deps / ("lib" + name + "-" + matches[0].parent.name.rsplit("-", 1)[1] + ".rlib")
flags = []
for p in sorted((deps.parent / "build").glob("*/out")):
    if any(p.glob("*.a")):
        flags += ["-L", "native=" + str(p)]
record = {"compiler_source_commit": "f5056ffefdbeb196768c4135d8f6a4769c05977e", "libraries": {name: {"path": str(p), "sha256": hashlib.sha256(p.read_bytes()).hexdigest()} for name, p in libs.items()}, "tests": []}
work = root / "target/probe/closure-ci"
work.mkdir(parents=True, exist_ok=True)
for name in ["portable_calling_convention", "portable_inline_predicates"]:
    source = root / "crates/suss-compile/tests" / (name + ".rs")
    command = ["rustup", "run", "stable", "rustc", "--edition=2024", "--test", str(source), "-L", "dependency=" + str(deps), *flags]
    for dep, path in libs.items():
        command += ["--extern", dep + "=" + str(path)]
    command += ["-o", str(work / name)]
    subprocess.run(command, check=True)
    run = [str(work / name), "--test-threads=2"]
    with (out / (name + ".log")).open("w") as log:
        result = subprocess.run(run, stdout=log, stderr=subprocess.STDOUT)
    record["tests"].append({"source": str(source.relative_to(root)), "sha256": hashlib.sha256(source.read_bytes()).hexdigest(), "compile": command, "run": run, "exit_code": result.returncode})
    (out / "commands.json").write_text(json.dumps(record, indent=2) + "\n")
    print((out / (name + ".log")).read_text(), flush=True)
    if result.returncode:
        raise SystemExit(result.returncode)
