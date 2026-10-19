#!/usr/bin/env python3
"""Retain bootstrap reproduction bytes; this is not an artifact validation gate."""
import hashlib
import json
import shutil
import subprocess
import sys
from pathlib import Path

NAMES = tuple(f"{phase}.{suffix}" for phase in ("runtime", "macro") for suffix in ("wasm", "json"))


def git(root, *args):
    return subprocess.check_output(["git", "-C", str(root), *args]).strip()


def retain(root, first, second, destination):
    root, first, second, destination = map(Path, (root, first, second, destination))
    for source in (first, second):
        for name in NAMES:
            if not (source / name).is_file():
                raise FileNotFoundError(source / name)
    source = {
        "head": git(root, "rev-parse", "HEAD").decode("ascii"),
        "tree": git(root, "rev-parse", "HEAD^{tree}").decode("ascii"),
        "working_tree_clean": not git(root, "status", "--porcelain", "--untracked-files=normal"),
    }
    # An explicit fresh destination prevents replacing unrelated prior evidence.
    destination.mkdir(parents=True, exist_ok=False)
    files = {}
    for label, directory in (("first", first), ("second", second)):
        (destination / label).mkdir()
        for name in NAMES:
            target = destination / label / name
            shutil.copyfile(directory / name, target)
            data = target.read_bytes()
            files[f"{label}/{name}"] = {"sha256": hashlib.sha256(data).hexdigest(), "bytes": len(data)}
    repeated = all(files[f"first/{name}"] == files[f"second/{name}"] for name in NAMES)
    shipped = all(
        (root / "runtime" / "bootstrap" / name).is_file()
        and hashlib.sha256((root / "runtime" / "bootstrap" / name).read_bytes()).hexdigest()
        == files[f"first/{name}"]["sha256"]
        for name in NAMES
    )
    receipt = {
        "scope": "Generated file bytes and source receipt only; no validation, execution or acceptance claim.",
        "source": source,
        "files": files,
        "reproductions_match": repeated,
        "shipped_matches_first": shipped,
    }
    # Publish last, so an interrupted copy has no complete receipt.
    temporary = destination / ".receipt.json.tmp"
    temporary.write_text(json.dumps(receipt, indent=2) + "\n")
    temporary.replace(destination / "receipt.json")
    return receipt


if __name__ == "__main__":
    if len(sys.argv) != 4:
        raise SystemExit("usage: bootstrap_diagnostics.py FIRST SECOND NEW_DESTINATION")
    retain(Path(__file__).resolve().parents[1], *sys.argv[1:])
