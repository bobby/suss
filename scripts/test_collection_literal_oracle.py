"""Keep the literal oracle's deliberate variances exact and visible."""
import contextlib
import copy
import io
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
import collection_literal_oracle as oracle


class LiteralOracleTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.corpus = Path(self.directory.name) / 'cases.json'
        self.observations = Path(self.directory.name) / 'observations.json'
        self.data = oracle.load()
        self.corpus.write_text(json.dumps(self.data))
        self.reference = {'schema': 1, 'upstream': oracle.PIN, 'cases': [
            {'id': c['id'], 'value': c['reference']} for c in self.data['cases']]}
        self.observations.write_text(json.dumps(self.reference))
        self.addCleanup(patch.stopall)
        patch.object(oracle, 'CORPUS', self.corpus).start()
        patch.object(oracle, 'OBSERVATIONS', self.observations).start()

    def test_exact_variances_are_reported_separately(self):
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            oracle.compare()
        self.assertIn('15 shared values, 4 explicit contract variances; 0 skips', output.getvalue())

    def test_suss_value_cannot_replace_reference_variance(self):
        changed = copy.deepcopy(self.reference)
        index = next(i for i, c in enumerate(self.data['cases']) if c['variance'])
        changed['cases'][index]['value'] = self.data['cases'][index]['expected']
        self.observations.write_text(json.dumps(changed))
        with self.assertRaises(ValueError):
            oracle.compare()

    def test_missing_duplicate_and_changed_reference_cases_fail(self):
        for mutation in ('missing', 'duplicate', 'changed'):
            with self.subTest(mutation=mutation):
                changed = copy.deepcopy(self.reference)
                if mutation == 'missing':
                    changed['cases'].pop()
                elif mutation == 'duplicate':
                    changed['cases'].append(changed['cases'][0])
                else:
                    changed['cases'][0]['value'] = {'tag': 'bool', 'value': False}
                self.observations.write_text(json.dumps(changed))
                with self.assertRaises(ValueError):
                    oracle.compare()

    def test_variance_requires_reason_and_actual_difference(self):
        index = next(i for i, c in enumerate(self.data['cases']) if c['variance'])
        for mutation in ('missing-reason', 'no-difference', 'skip'):
            with self.subTest(mutation=mutation):
                changed = copy.deepcopy(self.data)
                case = changed['cases'][index]
                if mutation == 'missing-reason':
                    case['variance'] = None
                elif mutation == 'no-difference':
                    case['reference'] = case['expected']
                else:
                    case['skip'] = True
                self.corpus.write_text(json.dumps(changed))
                with self.assertRaises(ValueError):
                    oracle.load()


if __name__ == '__main__':
    unittest.main()
