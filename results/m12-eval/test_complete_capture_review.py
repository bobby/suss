"""Adversarial checks of the real full capture; no synthetic passing rows."""
import copy
import gzip
import importlib.util
import json
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("guard", Path(__file__).with_name("review-complete-capture.py"))
guard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guard)

class CompleteCaptureGuard(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        folder = Path(__file__).with_name("eval-broader-after-symbol-and-var-contract") / "complete-capture"
        cls.old = gzip.decompress((folder / "parent-random-baseline.json.gz").read_bytes())
        cls.primary = (guard.ROOT / "tests/oracle/clojure-test-suite-observations.json").read_bytes()
        cls.previous = gzip.decompress((guard.ROOT / "results/m12-reader-followup/full-suite-after-random-and-assert/suite-capture/suite-observations.json.gz").read_bytes())
        cls.observed = json.loads(gzip.decompress((folder / "suite-capture/suite-observations.json.gz").read_bytes()))

    def check_candidate(self, observed=None, primary=None):
        return guard.review(self.primary if primary is None else primary, self.old, self.previous,
                           json.dumps(self.observed if observed is None else observed).encode())

    def test_actual_capture_allows_only_three_completed_namespaces(self):
        report = self.check_candidate()
        self.assertEqual(report["counts"]["pass"], 5354)
        self.assertEqual(report["raw_rows"], 5564)
        self.assertEqual(report["complete_bodies"]["clojure.core-test.symbol"]["host_matches"], 61)
        self.assertEqual(report["numeric_subsets"]["bigint-ratio"]["strict_host_matches"], 229)
        self.assertEqual(report["numeric_subsets"]["decimal"]["strict_host_matches"], 200)

    def test_actual_pre_symbol_failure_capture_remains_rejected(self):
        folder = Path(__file__).with_name("eval-broader-after-live-macro-publication") / "complete-capture"
        observed = json.loads(gzip.decompress((folder / "suite-capture/suite-observations.json.gz").read_bytes()))
        with self.assertRaises(AssertionError): self.check_candidate(observed=observed)

    def test_pass_verdict_cannot_hide_changed_numeric_operand(self):
        changed = copy.deepcopy(self.observed)
        row = next(r for r in changed["assertions"] if r["test"] == "clojure.core-test.eval/test-eval" and r["ordinal"] == 5)
        self.assertEqual(row["verdict"], "pass")
        row["operands"][1] = {"tag": "long", "value": "1"}
        with self.assertRaises(AssertionError): self.check_candidate(observed=changed)

    def test_missing_symbol_row_is_not_a_complete_body(self):
        changed = copy.deepcopy(self.observed)
        changed["assertions"] = [r for r in changed["assertions"] if not (r["test"] == "clojure.core-test.symbol/test-symbol" and r["ordinal"] == 61)]
        with self.assertRaises(AssertionError): self.check_candidate(observed=changed)

    def test_body_order_cannot_change(self):
        changed = copy.deepcopy(self.observed)
        positions = [i for i,r in enumerate(changed["assertions"]) if r["test"] == "clojure.core-test.eval/test-eval" and r["ordinal"] in [5,6]]
        a,b = positions
        changed["assertions"][a],changed["assertions"][b] = changed["assertions"][b],changed["assertions"][a]
        with self.assertRaises(AssertionError): self.check_candidate(observed=changed)

    def test_guest_verdict_must_still_match(self):
        changed = copy.deepcopy(self.observed)
        row = next(r for r in changed["assertions"] if r["test"] == "clojure.core-test.var-qmark/test-var?" and r["ordinal"] == 1)
        row["verdict"] = "fail"
        with self.assertRaises(AssertionError): self.check_candidate(observed=changed)

    def test_unrelated_namespace_failure_cannot_disappear(self):
        changed = copy.deepcopy(self.observed)
        row = next(r for r in changed["namespaces"] if r["namespace"] == "clojure.core-test.eq")
        row.clear(); row.update(namespace="clojure.core-test.eq", status="loaded")
        with self.assertRaises(AssertionError): self.check_candidate(observed=changed)

    def test_remaining_ifn_blocker_requires_exact_observed_diagnostic(self):
        changed = copy.deepcopy(self.observed)
        row = next(r for r in changed["namespaces"] if r["namespace"] == "clojure.core-test.ifn-qmark")
        row["diagnostic"] = row["diagnostic"].replace("promise", "unrelated-name")
        with self.assertRaises(AssertionError): self.check_candidate(observed=changed)

    def test_primary_cannot_change(self):
        with self.assertRaises(AssertionError): self.check_candidate(primary=self.primary+b" ")

if __name__ == "__main__": unittest.main()
