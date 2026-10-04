"""Malformed quoted-source projections must never become successful evidence."""
import copy
import json
from pathlib import Path
import unittest
from quote_asts_oracle import compare

class QuoteTransportTests(unittest.TestCase):
    def setUp(self):
        self.expected = json.loads((Path(__file__).resolve().parents[1]
            / 'tests/oracle/quote-ast-observations.json').read_text())
        self.actual = copy.deepcopy(self.expected['cases'])

    def test_candidate_structure_without_execution_claim(self):
        compare(self.expected, self.actual)

    def test_literal_boolean_cannot_be_numeric_one(self):
        self.actual[0][2][1][2] = 1
        with self.assertRaises(ValueError): compare(self.expected, self.actual)

    def test_absence_cannot_hide_a_value(self):
        self.actual[0][1][5][2] = ['keyword', ':const']
        with self.assertRaises(ValueError): compare(self.expected, self.actual)

    def test_missing_child_field_is_not_absence(self):
        self.actual[0][2].pop()
        with self.assertRaises(ValueError): compare(self.expected, self.actual)

    def test_missing_case_does_not_pass(self):
        self.actual.pop()
        with self.assertRaises(ValueError): compare(self.expected, self.actual)

    def test_quote_cannot_be_collapsed_into_const(self):
        self.actual[0][1][0][2] = ['keyword', ':const']
        with self.assertRaises(ValueError): compare(self.expected, self.actual)

    def test_datum_identity_loss_does_not_pass(self):
        self.actual[10][3][4] = False
        with self.assertRaises(ValueError): compare(self.expected, self.actual)

if __name__ == '__main__': unittest.main()
