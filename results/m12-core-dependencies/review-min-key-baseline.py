"""Allow only the independently observed removal of min-key's load failure."""
from pathlib import Path
import copy
import gzip
import hashlib
import json
import sys

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
import clojure_test_suite as suite

BASELINE = ROOT / "tests/oracle/clojure-test-suite-known-failures.json"
PRIMARY = ROOT / "tests/oracle/clojure-test-suite-observations.json"
PREVIOUS = ROOT / "results/m12-reader-followup/full-suite-after-reserved-core-head-transport/suite-capture/suite-observations.json.gz"
BASELINE_SHA = "88ccd83c7614da545c1c44b1c4ed090ac2c2e85ba9ad02e5b0d7290992b73f33"
PRIMARY_SHA = "449264ab6bca1dcf761d0204e4a64de4de91562e576615c4e84a63246839f4c7"
PREVIOUS_SHA = "10c7e6f5adf409eef8d162ed5fa5327f37a890bbfc8ef0849966d8f2e55380dd"
NAMESPACE = "clojure.core-test.min-key"
TEST = NAMESPACE + "/test-min-key"

def sha(raw):
    return hashlib.sha256(raw).hexdigest()

def review(old_raw, primary_raw, previous_raw, observed_raw, proposed):
    assert sha(old_raw) == BASELINE_SHA, "published parent baseline changed"
    assert sha(primary_raw) == PRIMARY_SHA, "primary changed"
    assert sha(previous_raw) == PREVIOUS_SHA, "parent capture changed"
    old = json.loads(old_raw)
    reference = json.loads(primary_raw)
    previous = json.loads(previous_raw)
    observed = json.loads(observed_raw)
    before_counts, before_failures = suite.compare(reference, previous)
    assert before_failures == old, "parent capture does not establish the baseline"
    counts, measured = suite.compare(reference, observed)
    expected = copy.deepcopy(old)
    assert expected["namespaces"].pop(NAMESPACE)["stage"] == "namespace"
    assert measured == proposed == expected, "unrelated failure/baseline change"
    assert [row["namespace"] for row in observed["namespaces"]] == reference["namespaces"]
    assert next(row for row in observed["namespaces"] if row["namespace"] == NAMESPACE) == {
        "namespace": NAMESPACE, "status": "loaded"}
    wanted = [row for row in reference["assertions"] if row["test"] == TEST]
    actual = [row for row in observed["assertions"] if row["test"] == TEST]
    assert [(row["test"], row["ordinal"]) for row in wanted] == [(TEST, n) for n in range(1, 50)]
    assert [(row["test"], row["ordinal"]) for row in actual] == [(TEST, n) for n in range(1, 50)]
    assert all(suite.decide(w, a) == (None, False) for w, a in zip(wanted, actual)), "all49 must be host matches"
    expected_counts = before_counts | {
        "pass": before_counts["pass"] + 49,
        "not-executed": before_counts["not-executed"] - 49,
        "namespaces-failed": before_counts["namespaces-failed"] - 1,
    }
    assert counts == expected_counts, "unexpected count/skip/guest change"
    return expected, counts

def main():
    out = Path(sys.argv[1]).resolve()
    receipt = json.loads((out / "suite-commands.json").read_text())
    assert receipt["complete"] and receipt["inputs_unchanged"]
    assert all((ROOT / p).is_file() and sha((ROOT / p).read_bytes()) == h
               for p, h in receipt["inputs"].items()), "capture inputs changed before review"
    assert next(c for c in receipt["commands"] if c["label"] == "suite-build")["exit_code"] == 0
    assert next(c for c in receipt["commands"] if c["label"] == "suite-lock")["exit_code"] == 0
    native = next(c for c in receipt["commands"] if c["label"] == "suite-run")
    assert native["exit_code"] == 101, "retain the original native baseline discrepancy"
    old_raw = BASELINE.read_bytes()
    observed_raw = (out / "suite-capture/suite-observations.json").read_bytes()
    expected, counts = review(old_raw, PRIMARY.read_bytes(), gzip.decompress(PREVIOUS.read_bytes()),
                              observed_raw, json.loads((out / "proposed-known-failures.json").read_text()))
    new_raw = (json.dumps(expected, indent=1) + "\n").encode()
    report = {
        "old_sha256": sha(old_raw), "new_sha256": sha(new_raw),
        "capture_sha256": sha(observed_raw), "counts": counts,
        "min_key_host_matches": 49, "original_native_capture_exit_code": 101,
        "qualification": "Only min-key's load failure is removed after actual complete execution; all other failures and primary/count/skip rules are preserved. Host comparison is not a native rerun or workspace pass.",
    }
    if "--write" in sys.argv[2:]:
        report_path = out / "reviewed-min-key-baseline-write.json"
        assert not report_path.exists()
        assert old_raw == (json.dumps(json.loads(old_raw), indent=1) + "\n").encode()
        BASELINE.write_bytes(new_raw)
        report_path.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))

if __name__ == "__main__":
    main()
