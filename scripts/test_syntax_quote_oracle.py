import json
import unittest
import syntax_quote_oracle as oracle


class SyntaxQuoteOracleTests(unittest.TestCase):
    def setUp(self):
        self.raw = (oracle.ROOT / 'tests/oracle/syntax-quote-observations.json').read_text()

    def test_actual_recorded_execution_compares(self):
        oracle.compare(self.raw, self.raw)

    def test_changes_signed_zero_and_invalid_shapes_fail(self):
        for value in (99, -0.0):
            data = json.loads(self.raw)
            data['cases'][0][1][2] = value
            with self.assertRaises(ValueError):
                oracle.compare(json.dumps(data), self.raw)
        for value in (True, None, '0', float('nan'), float('inf')):
            data = json.loads(self.raw)
            data['cases'][0][1][0] = value
            with self.assertRaises(ValueError):
                oracle.decode(json.dumps(data))

    def test_missing_cases_unknown_labels_pins_and_duplicates_fail(self):
        for key, value in [('cases', []), ('cases', [['unknown', [0]*18]]),
                           ('cases', [['execution', [0]*17]]), ('schema', True),
                           ('upstream', 'unknown'), ('extra', 0)]:
            data = json.loads(self.raw)
            data[key] = value
            with self.assertRaises(ValueError):
                oracle.decode(json.dumps(data))
        with self.assertRaises(ValueError):
            oracle.decode(self.raw.replace('"schema": 1', '"schema": 1, "schema": 1'))


if __name__ == '__main__':
    unittest.main()
