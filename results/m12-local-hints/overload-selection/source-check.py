"""Reconstruct measured source and require exact published file bytes."""
from pathlib import Path
import hashlib
import json
import subprocess
import tempfile

record = Path(__file__).resolve().parent
repo = record.parents[2]
receipt = json.loads((record / "receipts.json").read_text())
revision = receipt["published_source_revision"]
base = receipt["source_base"]
print("Measured base:", base)
print("Published source:", revision)
with tempfile.TemporaryDirectory(prefix="suss-overload-source-") as scratch:
    for path in receipt["committed_source_sha256"]:
        target = Path(scratch) / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(subprocess.check_output(["git", "show", base + ":" + path], cwd=repo))
    subprocess.run(["git", "apply", str(record / "final-source.patch")], cwd=scratch, check=True)
    for path, expected in receipt["committed_source_sha256"].items():
        measured = (Path(scratch) / path).read_bytes()
        published = subprocess.check_output(["git", "show", revision + ":" + path], cwd=repo)
        assert measured == published, path
        digest = hashlib.sha256(published).hexdigest()
        assert digest == expected, path
        print(digest, path)
print("All reconstructed source and test files exactly match the published revision.")
