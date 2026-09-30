import copy
import unittest
from comparison_oracle import validate_observations
from oracle_transport import PIN

class ComparisonObservationTests(unittest.TestCase):
    def setUp(self):
        self.expected = {'schema': 1, 'upstream': PIN,
                         'cases': [{'id': 'ordered', 'value': {'tag': 'bool', 'value': True}}],
                         'capture-divergences': [{'id': 'captured', 'value': {'tag': 'string', 'units': [84]}}]}

    def test_boolean_schema_does_not_equal_integer_schema(self):
        validate_observations(self.expected, self.expected)
        data = copy.deepcopy(self.expected)
        data['schema'] = True
        with self.assertRaises(ValueError):
            validate_observations(data, self.expected)

    def test_opaque_or_wrong_boolean_tags_are_rejected(self):
        for value in [{'tag': 'unknown'}, {'tag': 'bool', 'value': 1}]:
            data = copy.deepcopy(self.expected)
            data['cases'][0]['value'] = value
            with self.assertRaises(ValueError):
                validate_observations(data, self.expected)

    def test_divergence_cannot_be_dropped_or_changed_to_success(self):
        for capture in [[], [{'id': 'captured', 'value': {'tag': 'bool', 'value': True}}]]:
            data = copy.deepcopy(self.expected)
            data['capture-divergences'] = capture
            with self.assertRaises(ValueError):
                validate_observations(data, self.expected)

    def test_missing_duplicate_cases_and_wrong_error_text_are_rejected(self):
        for field, replacement in [('cases', []), ('cases', self.expected['cases'] * 2), ('capture-divergences', [{'id': 'captured', 'value': {'tag': 'string', 'units': [70]}}])]:
            data = copy.deepcopy(self.expected)
            data[field] = replacement
            with self.assertRaises(ValueError):
                validate_observations(data, self.expected)
