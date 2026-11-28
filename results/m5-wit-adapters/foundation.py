#!/usr/bin/env python3
"""Execute every original foundation command, retaining each actual outcome."""
import gzip
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]


def main():
    output = Path(sys.argv[1]).resolve()
    plan = json.loads((ROOT / "results/m5-wit-adapters/foundation-pinned/commands.json").read_text())
    outcomes = []
    for index, item in enumerate(plan):
        command = item["command"]
        start = time.monotonic()
        result = subprocess.run(command, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        log = f"{index:02}.log.gz"
        (output / log).write_bytes(gzip.compress(result.stdout, mtime=0))
        record = {"command": command, "exit_code": result.returncode,
                  "seconds": time.monotonic() - start, "log": log,
                  "log_sha256": hashlib.sha256(result.stdout).hexdigest()}
        outcomes.append(record)
        print(json.dumps(record), flush=True)
    (output / "commands.json").write_text(json.dumps(outcomes, indent=2) + "\n")
    return int(any(item["exit_code"] != 0 for item in outcomes))


if __name__ == "__main__":
    raise SystemExit(main())
