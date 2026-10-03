import copy
import unittest
from source_environment_oracle import ROOT, compare, parse


class SourceEnvironmentEvidenceTests(unittest.TestCase):
    def setUp(self):
        self.expected = parse((ROOT / 'tests/oracle/source-environment-observations.json').read_text())
        self.actual = copy.deepcopy(self.expected['cases'])
        self.result = self.expected['result'].copy()

    def test_exact_primary_capture(self):
        compare(self.expected, self.actual, self.result)

    def test_missing_duplicate_and_reordered_calls(self):
        for actual in (self.actual[:-1], self.actual + [self.actual[0]], self.actual[::-1]):
            with self.subTest(actual=actual[0][0]):
                with self.assertRaises(ValueError):
                    compare(self.expected, actual, self.result)

    def test_unknown_or_malformed_projection_is_not_opaque_success(self):
        for value in (['opaque', []], ['vector', {}], ['symbol', False], {'value': 1}):
            actual = copy.deepcopy(self.actual)
            actual[0][1] = value
            with self.subTest(value=value):
                with self.assertRaises(ValueError):
                    compare(self.expected, actual, self.result)

    def test_changed_context_or_initializer_requires_new_evidence(self):
        for index, value in ((3, 10), (5, ['keyword', ':expr'])):
            actual = copy.deepcopy(self.actual)
            actual[0][1][1][index] = value
            with self.subTest(index=index):
                with self.assertRaises(ValueError):
                    compare(self.expected, actual, self.result)

    def test_boolean_result_cannot_stand_in_for_integer(self):
        self.result[1] = True
        with self.assertRaises(ValueError):
            compare(self.expected, self.actual, self.result)

    def test_changed_pin_or_unknown_envelope_fields(self):
        for mutation in ({'upstream': 'unknown'}, {'schema': True}, {'extra': None}):
            expected = copy.deepcopy(self.expected)
            expected.update(mutation)
            with self.subTest(mutation=mutation):
                with self.assertRaises(ValueError):
                    compare(expected, self.actual, self.result)


if __name__ == '__main__':
    unittest.main()
