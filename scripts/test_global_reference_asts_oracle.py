"""Reject corrupt reference observations without relaxing field/type evidence."""
import copy
import json
from pathlib import Path
import unittest

from global_reference_asts_oracle import compare


class GlobalReferenceTransportTests(unittest.TestCase):
    def setUp(self):
        self.expected = json.loads((Path(__file__).resolve().parents[1]
            / "tests/oracle/global-reference-ast-observations.json").read_text())
        self.actual = copy.deepcopy(self.expected["cases"])

    def test_candidate_structure_is_valid_without_claiming_upstream_execution(self):
        compare(self.expected, self.actual)

    def test_numeric_one_cannot_replace_true_dynamic_metadata(self):
        self.actual[3][2][6][2] = 1
        with self.assertRaises(ValueError):
            compare(self.expected, self.actual)

    def test_absence_cannot_hide_a_retained_value(self):
        self.actual[0][2][8][2] = ["symbol", "number"]
        with self.assertRaises(ValueError):
            compare(self.expected, self.actual)

    def test_missing_field_is_not_an_absent_field(self):
        self.actual[4][2].pop()
        with self.assertRaises(ValueError):
            compare(self.expected, self.actual)

    def test_missing_case_cannot_be_claimed_complete(self):
        self.actual.pop()
        with self.assertRaises(ValueError):
            compare(self.expected, self.actual)


if __name__ == "__main__":
    unittest.main()
