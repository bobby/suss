#!/usr/bin/env python3
"""Retain a command's actual exit, raw log and before/after source identity."""
import gzip
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    if len(sys.argv) < 3:
        raise SystemExit("usage: run.py LABEL COMMAND [ARG ...]")
    label, *command = sys.argv[1:]
    if not label or Path(label).name != label:
        raise SystemExit("LABEL must be one directory name")
    output = Path(__file__).resolve().parent / label
    output.mkdir(exist_ok=False)
    paths = subprocess.check_output(
        ["git", "ls-files", "-co", "--exclude-standard"], cwd=ROOT, text=True
    ).splitlines()
    inputs = {
        name: digest(ROOT / name)
        for name in sorted(set(paths))
        if (ROOT / name).is_file() and output not in (ROOT / name).parents
    }
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    prerequisite_tools = {}
    if os.environ.get("SUSS_WASM_OPT"):
        binaryen = Path(os.environ["SUSS_WASM_OPT"])
        for path in (binaryen, binaryen.with_name("wasm-merge")):
            prerequisite_tools[str(path)] = digest(path)
    submodule = ROOT / "clojurescript"
    pinned_cljs = subprocess.check_output(["git", "-C", str(submodule), "rev-parse", "HEAD"], text=True).strip() if (submodule / ".git").exists() else None
    record = {
        "prerequisite_tools": prerequisite_tools,
        "pinned_clojurescript": pinned_cljs,
        "cargo_version": subprocess.check_output(["cargo", "--version"], text=True).strip(),
        "rustc_version": subprocess.check_output(["rustc", "--version"], text=True).strip(),
        "command": command,
        "head": head,
        "inputs": inputs,
        "runner_sha256": digest(Path(__file__)),
        "cgroup": Path("/proc/self/cgroup").read_text(),
        "environment": {key: os.environ[key] for key in [
            "CARGO_TARGET_DIR", "CARGO_BUILD_JOBS", "RUSTUP_TOOLCHAIN",
            "SUSS_BUILD_DIR", "SUSS_WASM_OPT", "BINARYEN_CORES", "TMPDIR",
        ] if key in os.environ},
        "complete": False,
    }
    receipt = output / "command.json"
    receipt.write_text(json.dumps(record, indent=2) + "\n")
    start = time.monotonic()
    raw_log = output / "command.log"
    with raw_log.open("wb") as log:
        result = subprocess.run(command, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT)
    record.update(
        exit_code=result.returncode,
        seconds=time.monotonic() - start,
        log_sha256=digest(raw_log),
        changed_inputs=[name for name, sha in inputs.items()
                        if not (ROOT / name).is_file() or digest(ROOT / name) != sha],
        head_unchanged=subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True
        ).strip() == head,
        prerequisites_unchanged=all(Path(name).is_file() and digest(Path(name)) == sha
                                    for name, sha in prerequisite_tools.items()),
        complete=True,
    )
    (output / "command.log.gz").write_bytes(gzip.compress(raw_log.read_bytes(), mtime=0))
    raw_log.unlink()
    receipt.write_text(json.dumps(record, indent=2) + "\n")
    raw_receipt = receipt.read_bytes()
    (output / "command.json.gz").write_bytes(gzip.compress(raw_receipt, mtime=0))
    summary = {key: value for key, value in record.items() if key != "inputs"}
    summary.update(receipt_sha256=hashlib.sha256(raw_receipt).hexdigest(), inputs_count=len(inputs))
    (output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    receipt.unlink()
    print(json.dumps(summary), flush=True)
    return result.returncode


if __name__ == "__main__":
    raise SystemExit(main())
