import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
import string_field_oracle as helper

class StringFieldOracleTests(unittest.TestCase):
    def test_closed_order_rejects_missing_reordered_duplicate_and_unknown(self):
        original = json.loads(helper.CORPUS.read_text())
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'cases.json'
            for cases in [original['cases'][:-1], list(reversed(original['cases'])),
                          original['cases'] + [original['cases'][0]],
                          [dict(case, id='unknown') if i == 0 else case
                           for i, case in enumerate(original['cases'])]]:
                path.write_text(json.dumps(dict(original, cases=cases)))
                with patch.object(helper, 'CORPUS', path), self.assertRaises(ValueError):
                    helper.corpus()

    def test_paths_restore_on_both_outcomes(self):
        old = helper.oracle.CORPUS, helper.oracle.OBSERVATIONS
        with helper.paths():
            self.assertEqual(helper.oracle.CORPUS, helper.CORPUS)
        self.assertEqual((helper.oracle.CORPUS, helper.oracle.OBSERVATIONS), old)
        with self.assertRaises(RuntimeError):
            with helper.paths():
                raise RuntimeError('failure')
        self.assertEqual((helper.oracle.CORPUS, helper.oracle.OBSERVATIONS), old)

    def test_raw_returns_have_exact_types(self):
        cases = {case['id']: case for case in helper.corpus()['cases']}
        self.assertEqual(cases['str-raw-number-return']['expected']['tag'], 'f64')
        self.assertEqual(cases['str-raw-nil-return']['expected'], {'tag': 'nil'})
        self.assertIn('(let [f str]', cases['str-raw-number-return']['source'])

if __name__ == '__main__':
    unittest.main()
