import copy
import json
from pathlib import Path
import unittest
from suite_baseline_log import project

ROOT = Path(__file__).resolve().parent.parent
EVIDENCE = ROOT / 'docs/compatibility/upstream-suite-baseline/evidence'


class SuiteLogProjectionTests(unittest.TestCase):
    def setUp(self):
        self.original = json.loads((EVIDENCE / 'original-known-failures.json').read_text())
        self.reference = json.loads((ROOT / 'tests/oracle/clojure-test-suite-observations.json').read_text())
        self.log = (EVIDENCE / 'pr231-integrated-head-suite.log').read_text()

    def test_exact_baseline_and_all_assertion_classifications(self):
        baseline, receipt = project(self.original, self.reference, self.log)
        self.assertEqual(baseline, json.loads((ROOT / 'tests/oracle/clojure-test-suite-known-failures.json').read_text()))
        self.assertEqual(len(receipt['assertions']), 5834)
        self.assertEqual(receipt['counts']['not-executed'], 5684)
        self.assertEqual(receipt['counts']['pass'], 150)
        self.assertEqual(receipt['counts']['fail'], 20)
        self.assertEqual(baseline['skip-mismatches'], self.original['skip-mismatches'])
        self.assertEqual(baseline['assertions'], {})
        self.assertEqual(baseline['tests'], {})
        self.assertNotIn('clojure.core-test.fnil', baseline['namespaces'])
        self.assertNotIn('clojure.core-test.get', baseline['namespaces'])
        self.assertIn('Unresolved Runtime name range', baseline['namespaces']['clojure.core-test.group-by']['diagnostic'])
        self.assertIn('Unresolved Runtime name with-out-str', baseline['namespaces']['clojure.core-test.with-out-str']['diagnostic'])
        self.assertEqual(sum(r['classification'] == 'not-executed' for r in receipt['assertions']), 5684)
        self.assertEqual(sum(r['classification'] == 'pass' for r in receipt['assertions']), 150)

    def test_missing_duplicate_or_changed_delta_fails_closed(self):
        line = next(x for x in self.log.splitlines() if 'namespaces clojure.core-test.fnil:' in x)
        for log in [self.log.replace(line, ''), self.log + '\n' + line,
                    self.log.replace('name fnil at bytes 222..1452', 'name fnil at bytes 222..1453'),
                    self.log.replace('pass=150', 'pass=5684')]:
            with self.assertRaises(ValueError):
                project(self.original, self.reference, log)
        # A diagnostic-only change is not inferable from the counters: dropping its
        # log line still yields matching counts, but only by keeping the stale
        # original diagnostic. The projection documents this blind spot rather
        # than fabricating the advance.
        group_by = next(x for x in self.log.splitlines() if 'namespaces clojure.core-test.group-by:' in x)
        baseline, receipt = project(self.original, self.reference, self.log.replace(group_by, ''))
        self.assertIn('Unresolved Runtime name group-by', baseline['namespaces']['clojure.core-test.group-by']['diagnostic'])
        self.assertEqual(receipt['counts'], {'fail': 20, 'guest-judged': 0, 'namespaces-failed': 233,
                                             'not-executed': 5684, 'pass': 150, 'skipped': 0})

    def test_stale_original_unknown_identity_and_incomplete_summary_rejected(self):
        stale = copy.deepcopy(self.original)
        stale['namespaces'].pop('clojure.core-test.fnil')
        with self.assertRaises(ValueError):
            project(stale, self.reference, self.log)
        with self.assertRaises(ValueError):
            project(self.original, self.reference,
                    self.log.replace('namespaces clojure.core-test.group-by:', 'namespaces clojure.core-test.not-a-namespace:'))
        with self.assertRaises(ValueError):
            project(self.original, self.reference, self.log.replace('guest-judged=0 ', ''))


if __name__ == '__main__':
    unittest.main()