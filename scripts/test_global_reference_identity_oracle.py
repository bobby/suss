"""Corruption checks for independently executed declaration-identity evidence."""
import copy
import unittest
from global_reference_identity_oracle import EXPECTED, compare


class IdentityEvidenceTests(unittest.TestCase):
    def evidence(self):
        return copy.deepcopy(EXPECTED), copy.deepcopy([EXPECTED[:2], EXPECTED[2:], 17])

    def test_actual_pinned_and_executed_rows_match(self):
        compare(*self.evidence())

    def test_numeric_one_cannot_replace_same_revision_boolean(self):
        rows, actual = self.evidence()
        rows[1][1] = 1
        with self.assertRaises(ValueError):
            compare(rows, actual)

    def test_distinct_revision_must_remain_distinct(self):
        rows, actual = self.evidence()
        actual[1][0][1] = True
        with self.assertRaises(ValueError):
            compare(rows, actual)

    def test_live_value_and_source_documents_remain_observed(self):
        rows, actual = self.evidence()
        actual[2] = 0
        with self.assertRaises(ValueError):
            compare(rows, actual)
        rows, actual = self.evidence()
        rows[2][2] = 'before'
        with self.assertRaises(ValueError):
            compare(rows, actual)
