import json
import unittest
import chunk_effect_oracle as oracle


class ChunkEffectOracleTests(unittest.TestCase):
    def setUp(self):
        self.raw = oracle.CORPUS.read_text()
        self.corpus = oracle.decode(self.raw)
        self.expected = dict(schema=1, upstream=oracle.PIN,
            cases=[dict(id=c['id'], value=c['expected']) for c in self.corpus['cases']])
        self.actual = json.dumps(self.expected)

    def test_complete_observations_compare(self):
        oracle.compare(self.actual, self.raw)

    def test_missing_reordered_duplicate_extra_fields_fail(self):
        for cases in [self.expected['cases'][:-1], list(reversed(self.expected['cases'])),
                      [self.expected['cases'][0]] * 9]:
            with self.assertRaises(ValueError):
                oracle.compare(json.dumps(dict(self.expected, cases=cases)), self.raw)
        with self.assertRaises(ValueError):
            oracle.compare(json.dumps(dict(self.expected, extra=1)), self.raw)
        with self.assertRaises(ValueError):
            oracle.decode(self.actual.replace('"schema": 1', '"schema": 1, "schema": 1'), True)

    def test_trace_order_bits_missing_effect_and_retry_fail(self):
        for mutate in [
            lambda v: v['cases'][0]['value']['items'][2]['items'][2]['items'].reverse(),
            lambda v: v['cases'][0]['value']['items'][1]['items'][2]['items'].pop(),
            lambda v: v['cases'][4]['value']['items'][4]['items'][2]['items'].pop(32),
            lambda v: v['cases'][0]['value']['items'][1]['items'][0].update(bits='8000000000000000'),
        ]:
            actual = json.loads(self.actual)
            mutate(actual)
            with self.assertRaises(ValueError):
                oracle.compare(json.dumps(actual), self.raw)

    def test_invalid_tag_float_or_schema_fail(self):
        for node in [{'tag':'opaque'}, {'tag':'f64','bits':'0'}, {'tag':'bool','value':1}]:
            actual = json.loads(self.actual)
            actual['cases'][0]['value']['items'][0] = node
            with self.assertRaises(ValueError):
                oracle.compare(json.dumps(actual), self.raw)
        with self.assertRaises(ValueError):
            oracle.decode(json.dumps(dict(self.corpus, schema=True)))
        with self.assertRaises(ValueError):
            oracle.decode(json.dumps(dict(self.corpus, upstream='unknown')))


if __name__ == '__main__':
    unittest.main()
