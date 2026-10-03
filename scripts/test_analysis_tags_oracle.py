"""Reject missing or softened reference evidence before certifying a fresh run."""
import copy
import unittest
from analysis_tags_oracle import ROOT, compare, parse


class AnalysisTagsEvidenceTests(unittest.TestCase):
    def setUp(self):
        self.expected = parse((ROOT / 'tests/oracle/analysis-tag-observations.json').read_text())
        self.actual = copy.deepcopy(self.expected['cases'])
        self.result = copy.deepcopy(self.expected['result'])

    def test_exact_capture(self):
        compare(self.expected, self.actual, self.result)

    def test_missing_duplicate_reordered_observations(self):
        for actual in (self.actual[:-1], self.actual + [self.actual[0]], self.actual[::-1]):
            with self.subTest(actual=actual[0][0]):
                with self.assertRaises(ValueError):
                    compare(self.expected, actual, self.result)

    def test_absence_and_present_nil_remain_distinct(self):
        # fn-unknown has present nil :inferred-ret-tag. Do not erase presence.
        self.actual[26][1][3][1] = False
        with self.assertRaises(ValueError):
            compare(self.expected, self.actual, self.result)

    def test_storage_value_cannot_replace_inferred_number(self):
        self.actual[11][1][1][2] = ['symbol', 'string']
        with self.assertRaises(ValueError):
            compare(self.expected, self.actual, self.result)

    def test_unknown_fields_and_values(self):
        for field in (['unknown', True, None], ['tag', False, ['symbol', 'number']],
                      ['tag', True, ['opaque', 'number']], ['tag', 1, None]):
            actual = copy.deepcopy(self.actual)
            actual[0][1][1] = field
            with self.subTest(field=field):
                with self.assertRaises(ValueError):
                    compare(self.expected, actual, self.result)

    def test_executed_projections_require_exact_types_and_values(self):
        for value in (True, '1', 2):
            result = self.result.copy()
            result[6] = value
            with self.subTest(value=value):
                with self.assertRaises(ValueError):
                    compare(self.expected, self.actual, result)

    def test_invalid_json_is_rejected(self):
        for text in ('{"cases":[],"cases":[]}', 'NaN', 'Infinity'):
            with self.subTest(text=text):
                with self.assertRaises(ValueError):
                    parse(text)


if __name__ == '__main__':
    unittest.main()
