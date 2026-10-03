import json
from pathlib import Path
import unittest
import lazy_sequence_oracle as oracle


class LazySequenceOracleTests(unittest.TestCase):
    def setUp(self):
        self.raw = (oracle.ROOT / 'tests/oracle/lazy-sequence-observations.json').read_text()
        self.data = json.loads(self.raw)

    def test_actual_recorded_observations_compare(self):
        oracle.compare(self.raw, self.raw)

    def test_changed_values_and_signed_zero_fail(self):
        for value in (99, -0.0):
            data = json.loads(self.raw)
            data['cases'][0][1][0] = value
            with self.assertRaises(ValueError):
                oracle.compare(json.dumps(data), self.raw)

    def test_unknown_shapes_and_non_numbers_fail(self):
        for value in (True, None, '0', float('nan'), float('inf')):
            data = json.loads(self.raw)
            data['cases'][0][1][0] = value
            with self.assertRaises(ValueError):
                oracle.decode(json.dumps(data))
        for key, value in [('schema', True), ('upstream', 'unknown'), ('extra', 1)]:
            data = json.loads(self.raw)
            data[key] = value
            with self.assertRaises(ValueError):
                oracle.decode(json.dumps(data))

    def test_case_identity_order_width_and_duplicates_fail(self):
        for cases in (list(reversed(self.data['cases'])), self.data['cases'][:1],
                      [self.data['cases'][0], self.data['cases'][0]], [['realize', []], self.data['cases'][1]]):
            data = dict(self.data, cases=cases)
            with self.assertRaises(ValueError):
                oracle.decode(json.dumps(data))
        with self.assertRaises(ValueError):
            oracle.decode(self.raw.replace('"schema": 1', '"schema": 1, "schema": 1'))


if __name__ == '__main__':
    unittest.main()
