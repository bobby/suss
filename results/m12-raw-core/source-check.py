"""Reconstruct measured source, then compare exact working or revision bytes."""
from pathlib import Path
import hashlib
import json
import subprocess
import sys
import tempfile
record = Path(__file__).resolve().parent
repo = record.parents[1]
inputs = json.loads((record / "source-inputs.json").read_text())
revision = sys.argv[1] if len(sys.argv) > 1 else None
patch = record / "source.patch"
assert hashlib.sha256(patch.read_bytes()).hexdigest() == inputs["patch_sha256"]
with tempfile.TemporaryDirectory(prefix="suss-raw-core-source-") as scratch:
    for path in inputs["sha256"]:
        if path in inputs["new_files"]:
            continue
        target = Path(scratch) / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(subprocess.check_output(["git", "show", inputs["base"] + ":" + path], cwd=repo))
    subprocess.run(["git", "apply", str(patch)], cwd=scratch, check=True)
    for path, expected in inputs["sha256"].items():
        measured = (Path(scratch) / path).read_bytes()
        actual = subprocess.check_output(["git", "show", revision + ":" + path], cwd=repo) if revision else (repo / path).read_bytes()
        assert measured == actual, path
        digest = hashlib.sha256(actual).hexdigest()
        assert digest == expected, path
        print(digest, path)
print("All7 measured input files exactly match the reconstructed source.")
