"""Adversarial review of an actual full capture; no synthetic passing rows."""
import copy
import gzip
import importlib.util
import json
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('guard', Path(__file__).with_name('review-random-baseline.py'))
guard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guard)

class BaselineGuard(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.old = gzip.decompress(Path(__file__).with_name('random-parent-baseline.json.gz').read_bytes())
        cls.primary = guard.PRIMARY.read_bytes()
        cls.previous = gzip.decompress(guard.PREVIOUS.read_bytes())
        folder = guard.ROOT/'results/m12-reader-followup/full-suite-after-random-and-assert'
        cls.observed = json.loads(gzip.decompress((folder/'suite-capture/suite-observations.json.gz').read_bytes()))
        cls.proposed = json.loads((folder/'proposed-known-failures.json').read_text())

    def check_candidate(self, observed=None, proposed=None, primary=None):
        return guard.review(self.old, self.primary if primary is None else primary, self.previous,
                            json.dumps(self.observed if observed is None else observed).encode(),
                            self.proposed if proposed is None else proposed)

    def test_actual_complete_capture_allows_only_the_scoped_change(self):
        _, counts, bodies = self.check_candidate()
        self.assertEqual(counts['pass'], 5238)
        self.assertEqual(bodies['clojure.core-test.constantly']['guest_ordinals'], [1,31])

    def test_unrelated_failure_cannot_be_erased(self):
        changed = copy.deepcopy(self.proposed)
        del changed['assertions'][next(iter(changed['assertions']))]
        with self.assertRaises(AssertionError): self.check_candidate(proposed=changed)

    def test_missing_original_constantly_row_is_not_a_pass(self):
        changed = copy.deepcopy(self.observed)
        changed['assertions'] = [r for r in changed['assertions'] if not (r['test'].startswith('clojure.core-test.constantly/') and r['ordinal']==30)]
        with self.assertRaises(AssertionError): self.check_candidate(observed=changed)

    def test_changed_original_operand_is_not_a_pass(self):
        changed = copy.deepcopy(self.observed)
        row = next(r for r in changed['assertions'] if r['test'].startswith('clojure.core-test.constantly/') and r['ordinal']==2)
        row['operands'][0] = {'tag':'nil'}
        with self.assertRaises(AssertionError): self.check_candidate(observed=changed)

    def test_a_guest_failure_cannot_be_called_success(self):
        changed = copy.deepcopy(self.observed)
        row = next(r for r in changed['assertions'] if r['test'].startswith('clojure.core-test.lazy-seq/') and r['ordinal']==1)
        row['verdict'] = 'fail'
        with self.assertRaises(AssertionError): self.check_candidate(observed=changed)

    def test_missing_previous_success_is_rejected(self):
        changed = copy.deepcopy(self.observed)
        changed['assertions'] = [r for r in changed['assertions'] if not (r['test'].startswith('clojure.core-test.min-key/') and r['ordinal']==1)]
        with self.assertRaises(AssertionError): self.check_candidate(observed=changed)

    def test_primary_cannot_be_rewritten(self):
        with self.assertRaises(AssertionError): self.check_candidate(primary=self.primary+b' ')

if __name__=='__main__':unittest.main()
