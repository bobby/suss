"""Negative transport gates for the independent numeric conversion evidence."""
import copy
import unittest
import numeric_oracle as numeric


class NumericTransportTests(unittest.TestCase):
    def test_boolean_schema_cannot_equal_integer_schema(self):
        corpus = copy.deepcopy(numeric.load())
        corpus['schema'] = True
        with self.assertRaisesRegex(ValueError, 'schema'):
            numeric.validate(corpus)

    def test_duplicate_or_missing_samples_are_not_success(self):
        corpus = copy.deepcopy(numeric.load())
        corpus['cases'][1] = corpus['cases'][0]
        with self.assertRaisesRegex(ValueError, 'duplicate'):
            numeric.validate(corpus)
        corpus['cases'].pop()
        with self.assertRaisesRegex(ValueError, 'missing'):
            numeric.validate(corpus)

    def test_boolean_utf16_unit_and_wrong_result_tag_are_rejected(self):
        corpus = copy.deepcopy(numeric.load())
        corpus['cases'][0]['formatted']['units'] = [True]
        with self.assertRaises(ValueError):
            numeric.validate(corpus)
        corpus['cases'][0]['formatted'] = {'tag': 'nil'}
        with self.assertRaisesRegex(ValueError, 'tag'):
            numeric.validate(corpus)
