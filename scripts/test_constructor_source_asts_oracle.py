import copy
import json
import unittest

from constructor_source_asts_oracle import ROOT, compare


class ConstructorSourceObservations(unittest.TestCase):
    def setUp(self):
        self.corpus = json.loads((ROOT / "tests/oracle/constructor-source-ast-observations.json").read_text())
        self.rows = copy.deepcopy(self.corpus["cases"])
        self.executed = [list(map(list, zip(copy.deepcopy(self.rows), self.corpus["values"]))), self.corpus["effects"]]

    def test_unchanged_observations(self):
        compare(self.corpus, self.rows, self.executed)

    def test_missing_case_and_duplicate_identity(self):
        for rows in [self.rows[:-1], [self.rows[0]] * len(self.rows)]:
            with self.assertRaises(ValueError):
                compare(self.corpus, rows, self.executed)

    def test_missing_operation_and_reordered_child_edges(self):
        for mutate in [lambda ast: ast[0].__setitem__(0, ["op", False, None]),
                       lambda ast: ast[2].reverse()]:
            rows = copy.deepcopy(self.rows)
            mutate(rows[0][1])
            with self.assertRaises(ValueError):
                compare(self.corpus, rows, self.executed)

    def test_wrong_class_facts_and_wrong_values(self):
        rows = copy.deepcopy(self.rows)
        rows[0][2][1][2] = 3
        with self.assertRaises(ValueError):
            compare(self.corpus, rows, self.executed)
        self.executed[0][0][1] = [2, 1]
        with self.assertRaises(ValueError):
            compare(self.corpus, self.rows, self.executed)

    def test_effects_must_execute_once_in_order(self):
        for effects in [[], list(reversed(self.executed[1])), self.executed[1] * 2]:
            self.executed[1] = effects
            with self.assertRaises(ValueError):
                compare(self.corpus, self.rows, self.executed)

    def test_stale_corpus_and_wrong_schema(self):
        self.corpus["cases"][0][1][0][1][2] = ["symbol", "invented"]
        with self.assertRaises(ValueError):
            compare(self.corpus, self.rows, self.executed)
        self.corpus["schema"] = True
        with self.assertRaises(ValueError):
            compare(self.corpus, self.rows, self.executed)


if __name__ == "__main__":
    unittest.main()
