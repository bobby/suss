import copy
from pathlib import Path
import unittest
from declaration_environment_oracle import parse
from function_name_ast_oracle import compare


class FunctionNameObservations(unittest.TestCase):
    def setUp(self):
        # Recorded data tests the transport gate; this is not a fresh analyzer run.
        path = Path(__file__).resolve().parents[1] / "tests/oracle/function-name-ast-observations.json"
        self.corpus = parse(path.read_text())
        self.executed = copy.deepcopy(self.corpus["cases"])
        self.rows = [copy.deepcopy(case[0]) for case in self.executed]

    def test_recorded_transport(self):
        compare(self.corpus, self.rows, self.executed)

    def test_missing_case(self):
        with self.assertRaises(ValueError):
            compare(self.corpus, self.rows[:-1], self.executed)

    def test_missing_field(self):
        self.rows[0][1].pop()
        with self.assertRaises(ValueError):
            compare(self.corpus, self.rows, self.executed)

    def test_non_boolean_presence(self):
        self.rows[0][1][0] = None
        with self.assertRaises(ValueError):
            compare(self.corpus, self.rows, self.executed)

    def test_invalid_child_edges(self):
        self.rows[1][1][4] = ["methods", "local"]
        with self.assertRaises(ValueError):
            compare(self.corpus, self.rows, self.executed)

    def test_false_return_tag_is_not_nil(self):
        self.rows[4][1][17] = None
        with self.assertRaisesRegex(ValueError, "raw name observations differ"):
            compare(self.corpus, self.rows, self.executed)

    def test_shared_scope_identity_changes_are_not_hidden(self):
        self.rows[1][1][20] = False
        with self.assertRaisesRegex(ValueError, "raw name observations differ"):
            compare(self.corpus, self.rows, self.executed)

    def test_boolean_execution_is_not_number(self):
        self.executed[0][1] = True
        with self.assertRaises(ValueError):
            compare(self.corpus, self.rows, self.executed)

    def test_recorded_corpus_difference(self):
        self.corpus["cases"][0][1] = 43
        with self.assertRaisesRegex(ValueError, "recorded corpus"):
            compare(self.corpus, self.rows, self.executed)


if __name__ == "__main__":
    unittest.main()
