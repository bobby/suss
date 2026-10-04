import copy
import unittest
from method_recurrence_oracle import LABELS, compare
from oracle_transport import PIN


class MethodObservations(unittest.TestCase):
    def setUp(self):
        # Synthetic transport fixtures are not upstream semantic evidence.
        self.rows = [[label, [[True, None, True, True, "expr", False]] *
                      (2 if label == "multiple-methods" else 1)] for label in LABELS]
        self.executed = [[row, 42] for row in self.rows]
        self.corpus = {"schema": 1, "upstream": PIN, "cases": self.executed}

    def test_valid_transport(self):
        compare(self.corpus, self.rows, self.executed)

    def test_missing_case(self):
        with self.assertRaises(ValueError):
            compare(self.corpus, self.rows[:-1], self.executed)

    def test_false_is_not_nil_recurrence(self):
        rows = copy.deepcopy(self.rows)
        rows[0][1][0][1] = False
        with self.assertRaises(ValueError):
            compare(self.corpus, rows, self.executed)

    def test_missing_body_marker(self):
        rows = copy.deepcopy(self.rows)
        rows[0][1][0][2] = False
        with self.assertRaises(ValueError):
            compare(self.corpus, rows, self.executed)

    def test_raw_analyzer_node_difference(self):
        executed = copy.deepcopy(self.executed)
        executed[0][0][1][0][1] = True
        with self.assertRaisesRegex(ValueError, "raw observations differ"):
            compare(self.corpus, self.rows, executed)

    def test_boolean_execution_is_not_number(self):
        executed = copy.deepcopy(self.executed)
        executed[0][1] = True
        with self.assertRaises(ValueError):
            compare(self.corpus, self.rows, executed)

    def test_recorded_corpus_difference(self):
        corpus = copy.deepcopy(self.corpus)
        corpus["cases"][0][1] = 43
        with self.assertRaisesRegex(ValueError, "recorded corpus"):
            compare(corpus, self.rows, self.executed)


if __name__ == "__main__":
    unittest.main()
