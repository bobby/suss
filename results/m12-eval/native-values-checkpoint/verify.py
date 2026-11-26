#!/usr/bin/env python3
"""Verify archived actual terminals and current inputs across prose publication."""
import gzip
import hashlib
import io
import json
from pathlib import Path
import tarfile

here = Path(__file__).resolve().parent
root = here.parents[2]
sha = lambda data: hashlib.sha256(data).hexdigest()
summary = json.loads((here / "summary.json").read_text())
evidence = json.loads((here / "evidence.json").read_text())
source = json.loads((here / "source-overlay.json").read_text())
assert sha((here / "source-overlay.tar.gz").read_bytes()) == source["archive_sha256"]
with tarfile.open(fileobj=io.BytesIO(gzip.decompress((here / "source-overlay.tar.gz").read_bytes()))) as archive:
    archived = {member.name: sha(archive.extractfile(member).read()) for member in archive.getmembers()}
assert archived == source["files"]
assert all(sha((root / path).read_bytes()) == digest for path, digest in source["files"].items())
for row in evidence["archives"]:
    data = (here / row["archive"]).read_bytes()
    assert sha(data) == row["archive_sha256"], row["archive"]
    assert sha(gzip.decompress(data)) == row["sha256"], row["source"]
current = []
for name in ["native-immutable-metadata-identity-working", "native-values-ownership-abi-and-frontends-working"]:
    data = json.loads(gzip.decompress((here / (name + "--commands.json.gz")).read_bytes()))
    assert data["complete"] and data["inputs_unchanged"] and data["binaryen_unchanged"]
    assert data["head"] == summary["base_head"]
    for path, digest in data["inputs"].items():
        delta = summary["allowed_publication_delta"].get(path)
        if delta:
            assert digest == delta["before"], path
            digest = delta["after"]
        assert sha((root / path).read_bytes()) == digest, path
    for path, digest in data["binaryen"].items():
        assert sha(Path(path).read_bytes()) == digest, path
    current.extend(data["commands"])
assert len(current) == 10 and all(row["complete"] for row in current)
assert [row["name"] for row in current if row["exit_code"]] == ["complete-public-eval-regressions"]
assert summary["counts"]["public_eval_passed"] == 76
assert summary["counts"]["public_eval_failed"] == 1
print("Verified ten actual current terminals, source/fixtures/tools, exact overlay and explicit prose delta. Var/Namespace aggregate remains failing; no issue or promotion claim.")
