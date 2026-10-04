import copy
import json
import unittest
from pathlib import Path
from collection_source_asts_oracle import compare, node

class CollectionSourceOracleTests(unittest.TestCase):
    def setUp(self):
        self.corpus = json.loads((Path(__file__).resolve().parents[1] / "tests/oracle/collection-source-ast-observations.json").read_text())

    def test_fresh_rows(self):
        compare(self.corpus, self.corpus["cases"], 1)

    def test_effect_replay_and_bool_rejected(self):
        for effect in [0, 2, True]:
            with self.assertRaises(ValueError):
                compare(self.corpus, self.corpus["cases"], effect)

    def test_child_omission_rejected(self):
        row = copy.deepcopy(self.corpus["cases"][1][1])
        row[2] = []
        with self.assertRaises(ValueError):
            node(row)

    def test_child_cardinality_rejected(self):
        row = copy.deepcopy(self.corpus["cases"][1][1])
        row[2][0][2][0] = "storage"
        with self.assertRaises(ValueError):
            node(row)

    def test_case_loss_rejected(self):
        with self.assertRaises(ValueError):
            compare(self.corpus, self.corpus["cases"][:-1], 1)

    def test_absent_tag_with_value_rejected(self):
        row = copy.deepcopy(self.corpus["cases"][9][1])
        row[0][1][2] = False
        with self.assertRaises(ValueError):
            node(row)

    def test_source_value_change_rejected(self):
        rows = copy.deepcopy(self.corpus["cases"])
        rows[1][1][2][0][2][1][2][1][2] = 99
        with self.assertRaises(ValueError):
            compare(self.corpus, rows, 1)

    def test_unordered_primary_children_cannot_be_silently_reordered(self):
        # Native textual-order adaptation must not rewrite the pinned evidence.
        for index in [3, 5, 13]:  # map, set and factory set
            rows = copy.deepcopy(self.corpus["cases"])
            for edge in rows[index][1][2]:
                edge[2][1].reverse()
            with self.assertRaises(ValueError):
                compare(self.corpus, rows, 1)

    def test_unknown_schema_rejected(self):
        corpus = copy.deepcopy(self.corpus)
        corpus["schema"] = 1.0
        with self.assertRaises(ValueError):
            compare(corpus, corpus["cases"], 1)

if __name__ == "__main__":
    unittest.main()
