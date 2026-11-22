"""Assert the raw-field regression fails on the frozen 5b40654f compiler.

SUSS_EXISTING_TARGET selects its immutable compiled Cargo libraries.
"""
from pathlib import Path
import hashlib
import json
import os
import subprocess

root = Path(__file__).resolve().parents[2]
out = root / "results/m12-numeric-captures"
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
record = {"compiler_source_commit": "5b40654fcc63cdc59bc44f8090bdff8ad1338109", "libraries": {name: {"path": str(p), "sha256": hashlib.sha256(p.read_bytes()).hexdigest()} for name, p in libs.items()}, "tests": []}
work = root / "target/probe/numeric-captures-before"
work.mkdir(parents=True, exist_ok=True)
for name in ["portable_numeric_captures"]:
    source = root / "crates/suss-compile/tests" / (name + ".rs")
    command = ["rustup", "run", "stable", "rustc", "--edition=2024", "--test", str(source), "-L", "dependency=" + str(deps), *flags]
    for dep, path in libs.items():
        command += ["--extern", dep + "=" + str(path)]
    command += ["-o", str(work / name)]
    subprocess.run(command, check=True)
    run = [str(work / name), "numeric_only_captures_use_raw_fields_in_ordinary_named_and_general_closures", "--exact", "--test-threads=2"]
    with (out / ("before-codegen.log")).open("w") as log:
        result = subprocess.run(run, stdout=log, stderr=subprocess.STDOUT)
    record["tests"].append({"source": str(source.relative_to(root)), "sha256": hashlib.sha256(source.read_bytes()).hexdigest(), "compile": command, "run": run, "exit_code": result.returncode})
    (out / "before-codegen-command.json").write_text(json.dumps(record, indent=2) + "\n")
    print((out / ("before-codegen.log")).read_text(), flush=True)
    assert result.returncode == 101, "the parent must fail the raw-field regression"
