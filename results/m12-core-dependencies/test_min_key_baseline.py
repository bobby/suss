"""Adversarial guard checks; combined captures here are test fixtures only."""
import copy
import gzip
import importlib.util
import json
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("guard", Path(__file__).with_name("review-min-key-baseline.py"))
guard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guard)

class BaselineGuard(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.old = gzip.decompress(Path(__file__).with_name("min-key-parent-baseline.json.gz").read_bytes())
        cls.primary = guard.PRIMARY.read_bytes()
        cls.previous = gzip.decompress(guard.PREVIOUS.read_bytes())
        # Use actual parent/focused rows to exercise the guard, never as a
        # substitute for the required native full-suite capture.
        focused = json.loads(gzip.decompress((Path(__file__).parent /
            "complete-min-key-body/capture/min-key-observations.json.gz").read_bytes()))
        observed = json.loads(cls.previous)
        observed["assertions"].extend(focused["assertions"])
        observed["namespaces"] = [
            {"namespace": guard.NAMESPACE, "status": "loaded"} if row["namespace"] == guard.NAMESPACE else row
            for row in observed["namespaces"]]
        cls.observed = observed
        cls.proposed = json.loads(cls.old)
        del cls.proposed["namespaces"][guard.NAMESPACE]

    def check_candidate(self, observed=None, proposed=None, primary=None):
        return guard.review(self.old, self.primary if primary is None else primary, self.previous,
                            json.dumps(self.observed if observed is None else observed).encode(),
                            self.proposed if proposed is None else proposed)

    def test_actual_rows_allow_only_the_scoped_fixture_change(self):
        _, counts = self.check_candidate()
        self.assertEqual(counts["pass"], 5172)

    def test_unrelated_failure_cannot_be_erased(self):
        changed = copy.deepcopy(self.proposed)
        del changed["assertions"][next(iter(changed["assertions"]))]
        with self.assertRaises(AssertionError):
            self.check_candidate(proposed=changed)

    def test_missing_min_key_observation_cannot_be_called_passing(self):
        changed = copy.deepcopy(self.observed)
        changed["assertions"] = [r for r in changed["assertions"]
                                 if not (r["test"] == guard.TEST and r["ordinal"] == 49)]
        with self.assertRaises(AssertionError):
            self.check_candidate(observed=changed)

    def test_changed_min_key_operand_cannot_be_called_passing(self):
        changed = copy.deepcopy(self.observed)
        row = next(r for r in changed["assertions"] if r["test"] == guard.TEST)
        row["operands"][0] = {"tag": "nil"}
        with self.assertRaises(AssertionError):
            self.check_candidate(observed=changed)

    def test_primary_cannot_be_rewritten(self):
        with self.assertRaises(AssertionError):
            self.check_candidate(primary=self.primary + b" ")

if __name__ == "__main__":
    unittest.main()
