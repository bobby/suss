"""Reject malformed or altered genuine control observations."""
import copy
import unittest

from control_source_asts_oracle import ROOT, PAYLOAD_PATHS, compare, compare_raw
from declaration_environment_oracle import parse


class ControlObservations(unittest.TestCase):
    def setUp(self):
        self.corpus = parse((ROOT / "tests/oracle/control-source-ast-observations.json").read_text())
        self.rows = copy.deepcopy(self.corpus["cases"])

    def test_recorded_primary_shape_is_valid(self):
        compare(self.corpus, self.rows, 2)

    def test_missing_case_is_rejected(self):
        self.rows.pop()
        with self.assertRaises(ValueError):
            compare(self.corpus, self.rows, 2)

    def test_changed_case_order_is_rejected(self):
        self.rows[0], self.rows[1] = self.rows[1], self.rows[0]
        with self.assertRaises(ValueError):
            compare(self.corpus, self.rows, 2)

    def test_cleanup_value_and_type_are_exact(self):
        for effects in [1, 3, 2.0, True, None]:
            with self.subTest(effects=effects), self.assertRaises(ValueError):
                compare(self.corpus, self.rows, effects)

    def test_missing_declared_branch_is_rejected(self):
        self.rows[3][1][2].pop()
        with self.assertRaises(ValueError):
            compare(self.corpus, self.rows, 2)

    def test_present_nil_is_not_replaced_by_absence(self):
        otherwise = next(edge for edge in self.rows[5][1][2] if edge[0] == "else")[2][1]
        self.assertEqual(otherwise[1], [True, True, None])
        otherwise[1][0] = False
        with self.assertRaises(ValueError):
            compare(self.corpus, self.rows, 2)

    def test_child_cardinality_is_exact(self):
        self.rows[3][1][2][0][2][0] = "many"
        with self.assertRaises(ValueError):
            compare(self.corpus, self.rows, 2)

    def payload(self, path):
        value = self.rows
        for index in path:
            value = value[index]
        return value

    def test_two_consistent_counter_names_have_explicit_alpha_correspondence(self):
        for path in PAYLOAD_PATHS:
            self.payload(path)[1] = "e15746"
        compare(self.corpus, self.rows, 2)
        self.assertEqual(self.corpus["cases"][13][1][2][1][2][1][0][2][2][1][1][1][1],
                         ["symbol", "e16497"])

    def test_inconsistent_payload_or_user_name_is_rejected(self):
        for name in ["e15746", "error", "$exception34", "e", "e-1"]:
            with self.subTest(name=name):
                self.rows = copy.deepcopy(self.corpus["cases"])
                self.payload(PAYLOAD_PATHS[0])[1] = name
                with self.assertRaises(ValueError):
                    compare(self.corpus, self.rows, 2)

    def test_extra_private_use_is_not_normalized_away(self):
        self.rows[0][1][0][2][2] = ["symbol", "e16497"]
        with self.assertRaises(ValueError):
            compare(self.corpus, self.rows, 2)

    def test_raw_analyzer_node_comparison_does_not_allow_alpha_renaming(self):
        for path in PAYLOAD_PATHS:
            self.payload(path)[1] = "e15746"
        compare(self.corpus, self.rows, 2)
        with self.assertRaises(ValueError):
            compare_raw(self.corpus["cases"], self.rows)



if __name__ == "__main__":
    unittest.main()
