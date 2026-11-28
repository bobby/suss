import hashlib
import json
import random
import re
import subprocess
import time
from pathlib import Path

root = Path.cwd()
out = root / "target/probe/parse-long-oracle"
out.mkdir(exist_ok=True)
(out / "deps.edn").write_text('{:deps {org.clojure/clojure {:mvn/version "1.12.6"}}}\n')
primary = json.loads((root / "results/m12-context-constructors/bmp-digits.json").read_text())
starts = [unit for unit, value in primary if value == 0]
assert len(starts) == 37
cases = ["0", "9007199254740993", "9223372036854775807",
         "-9223372036854775808", "9223372036854775808", "bad",
         "+17", "١٧", "１７", " 17", "", "-0"]
for start in starts:
    cases.extend(chr(start + digit) for digit in range(10))
    for number in ["9223372036854775807", "-9223372036854775808",
                   "9223372036854775808", "-9223372036854775809",
                   "+17", "0" * 80 + "17"]:
        cases.append("".join(chr(start + int(char)) if char.isascii() and char.isdigit()
                             else char for char in number))
rng = random.Random(527)
for _ in range(160):
    text = str(rng.randrange(1 << 70))
    text = "".join(chr(rng.choice(starts) + int(char)) for char in text)
    cases.append(rng.choice(["", "+", "-"]) + text)
cases.extend([" ", "+", "-", "+-1", "--1", "1.0", "1/2", "0x10", "1N",
              "1M", "1e2", "١_٢", "１２ ３", "𝟙", "𝟙𝟚", "1\x00",
              "١a", "1:", " ١", "17\n", "−17", "+ 1", "1,000", "\t1"])
cases = list(dict.fromkeys(cases))
(out / "inputs.json").write_text(json.dumps(cases, indent=2, ensure_ascii=False) + "\n")
(out / "inputs.edn").write_text("[" + "\n".join(json.dumps(s, ensure_ascii=False) for s in cases) + "]\n")
(out / "oracle.clj").write_text("""(require '[clojure.edn :as edn])
(assert (= "1.12.6" (clojure-version)))
(prn {:clojure-version (clojure-version)})
(doseq [s (edn/read-string (slurp "inputs.edn"))]
  (prn [(parse-long s) (try (Long/parseLong s) (catch NumberFormatException _ :error))]))
""")
command = ["mise", "exec", "--", "clojure", "-M", "oracle.clj"]
start = time.monotonic()
run = subprocess.run(command, cwd=out, capture_output=True, text=True)
(out / "oracle.stdout").write_text(run.stdout)
(out / "oracle.stderr").write_text(run.stderr)
record = {
    "command": command, "seconds": time.monotonic() - start,
    "exit_code": run.returncode,
    "inputs_sha256": hashlib.sha256((out / "inputs.edn").read_bytes()).hexdigest(),
    "source_sha256": hashlib.sha256((out / "oracle.clj").read_bytes()).hexdigest(),
    "stdout_sha256": hashlib.sha256(run.stdout.encode()).hexdigest(),
}
(out / "receipt.json").write_text(json.dumps(record, indent=2) + "\n")
assert run.returncode == 0, run.stderr
lines = run.stdout.splitlines()
assert lines.pop(0) == '{:clojure-version "1.12.6"}'
assert len(lines) == len(cases), (len(lines), len(cases))
compiled = []
for text, line in zip(cases, lines):
    match = re.fullmatch(r"\[(-?[0-9]+|nil) (-?[0-9]+|:error)\]", line)
    assert match, line
    expected, throwing = match.groups()
    assert (throwing == ":error" and expected == "nil") or throwing == expected, line
    compiled.append(["(parse-long " + json.dumps(text, ensure_ascii=False) + ")", expected])
(out / "cases.json").write_text(json.dumps(compiled, indent=2, ensure_ascii=False) + "\n")
print(json.dumps({"cases": len(cases), "digit_families": len(starts), **record}))
