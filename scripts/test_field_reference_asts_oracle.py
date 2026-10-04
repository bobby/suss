import copy
import json
import unittest
from pathlib import Path
from field_reference_asts_oracle import compare, validate

class FieldOracleTests(unittest.TestCase):
    def setUp(self):
        self.corpus = json.loads((Path(__file__).resolve().parents[1] / "tests/oracle/field-reference-ast-observations.json").read_text())

    def test_exact_primary_rows(self):
        compare(self.corpus, self.corpus["cases"])

    def test_nil_cannot_replace_false(self):
        rows = copy.deepcopy(self.corpus["cases"])
        rows[4][2][2][2] = None
        with self.assertRaises(ValueError):
            compare(self.corpus, rows)

    def test_absent_tag_cannot_replace_present_nil(self):
        rows = copy.deepcopy(self.corpus["cases"])
        rows[0][2][5][1] = False
        with self.assertRaises(ValueError):
            compare(self.corpus, rows)

    def test_identity_loss_rejected(self):
        rows = copy.deepcopy(self.corpus["cases"])
        rows[0][3][1] = False
        with self.assertRaises(ValueError):
            compare(self.corpus, rows)

    def test_source_position_change_rejected(self):
        rows = copy.deepcopy(self.corpus["cases"])
        rows[0][2][8][2] += 1
        with self.assertRaises(ValueError):
            compare(self.corpus, rows)

    def test_absent_field_with_value_rejected(self):
        row = copy.deepcopy(self.corpus["cases"][0])
        row[1][2][2] = False
        with self.assertRaises(ValueError):
            validate(row)

    def test_missing_case_rejected(self):
        with self.assertRaises(ValueError):
            compare(self.corpus, self.corpus["cases"][:-1])

if __name__ == "__main__":
    unittest.main()
