"""Fail closed on concealed/mistyped lazy transformation evidence."""
import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import lazy_transformation_oracle as lazy


class LazyTransformationOracleTests(unittest.TestCase):
    def setUp(self):
        self.corpus = lazy.corpus()
        self.observations = {
            'schema': 1, 'upstream': self.corpus['upstream'],
            'cases': [{'id': c['id'], 'value': c['expected']}
                      for c in self.corpus['cases']],
        }

    def compare(self, observations):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'observations.json'
            path.write_text(json.dumps(observations))
            with patch.object(lazy, 'OBSERVATIONS', path), lazy.paths():
                lazy.oracle.compare()

    def test_full_shape_and_shared_paths_are_restored(self):
        saved = lazy.oracle.CORPUS, lazy.oracle.OBSERVATIONS
        self.compare(self.observations)
        self.assertEqual(saved, (lazy.oracle.CORPUS, lazy.oracle.OBSERVATIONS))
        with self.assertRaises(ValueError):
            self.compare(dict(self.observations, schema=True))
        self.assertEqual(saved, (lazy.oracle.CORPUS, lazy.oracle.OBSERVATIONS))

    def test_missing_reordered_duplicate_and_extra_cases_reject(self):
        cases = self.observations['cases']
        for changed in (cases[:-1], list(reversed(cases)),
                        [cases[0]] + cases, cases + [dict(cases[-1], id='hidden')]):
            with self.assertRaises(ValueError):
                self.compare(dict(self.observations, cases=changed))
        changed = copy.deepcopy(self.observations)
        changed['cases'][0]['extra'] = 'ignored failure'
        with self.assertRaises(ValueError):
            self.compare(changed)

    def test_changed_bits_boolean_number_and_unknown_tags_reject(self):
        for changed_value in ({'tag': 'f64', 'bits': '8000000000000000'},
                              {'tag': 'f64', 'bits': True},
                              {'tag': 'bool', 'value': 0},
                              {'tag': 'skip', 'reason': 'unknown'}):
            changed = copy.deepcopy(self.observations)
            changed['cases'][0]['value']['items'][0] = changed_value
            with self.assertRaises(ValueError):
                self.compare(changed)

    def test_corpus_identity_order_and_duplicate_json_fields_reject(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'corpus.json'
            with patch.object(lazy, 'CORPUS', path):
                for cases in (self.corpus['cases'][:-1],
                              list(reversed(self.corpus['cases']))):
                    path.write_text(json.dumps(dict(self.corpus, cases=cases)))
                    with self.assertRaises(ValueError):
                        lazy.corpus()
                path.write_text(json.dumps(self.corpus).replace(
                    '"schema": 1', '"schema": 1, "schema": 1'))
                with self.assertRaises(ValueError):
                    lazy.corpus()


if __name__ == '__main__':
    unittest.main()
