"""Reconstruct the exact measured source from its committed base and patch."""
from pathlib import Path
import argparse
import hashlib
import json
import subprocess
import tempfile

root = Path(__file__).resolve().parents[2]
artifacts = root / "results/m12-vector-nth-oracle"
parser = argparse.ArgumentParser()
parser.add_argument("--manifest", default="source-inputs.json")
parser.add_argument("--patch", default="source.patch")
parser.add_argument("--reconstructed-only", action="store_true")
args = parser.parse_args()
record = json.loads((artifacts / args.manifest).read_text())
with tempfile.TemporaryDirectory() as temporary:
    reconstructed = Path(temporary)
    for name in record["files"]:
        before = subprocess.run(["git", "show", record["base_commit"] + ":" + name], cwd=root, capture_output=True)
        if before.returncode == 0:
            path = reconstructed / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(before.stdout)
    subprocess.run(["git", "apply", "--check", str(artifacts / args.patch)], cwd=reconstructed, check=True)
    subprocess.run(["git", "apply", str(artifacts / args.patch)], cwd=reconstructed, check=True)
    for name, wanted in record["files"].items():
        for tree in ([reconstructed] if args.reconstructed_only else [root, reconstructed]):
            found = hashlib.sha256((tree / name).read_bytes()).hexdigest()
            assert found == wanted, (tree, name, found, wanted)
if args.reconstructed_only:
    print(f"All{len(record['files'])} reconstructed inputs match their frozen hashes.")
else:
    print(f"All{len(record['files'])} input files match the reconstructed source byte-for-byte.")
