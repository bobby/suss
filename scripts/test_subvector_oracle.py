"""Strict comparator regressions, independent of Cargo and compiler execution."""
import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import subvector_oracle as oracle

class SubvectorTransportTests(unittest.TestCase):
    def test_complete_fresh_shape(self):
        corpus = oracle.load()
        self.assertEqual(len(corpus['cases']), 34)
        self.assertEqual(len(oracle.errors()), 11)
        observed = {'schema':1, 'upstream':oracle.PIN, 'cases':[
            {'id':c['id'], 'value':c['expected']} for c in corpus['cases']] + [
            {'id':'error-'+c['id'], 'value':oracle.error_expected(c)} for c in oracle.errors()]}
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp)/'observations.json'
            path.write_text(json.dumps(observed))
            with patch.object(oracle, 'OBSERVATIONS', path):
                oracle.compare()
                for mutated in [dict(observed, cases=observed['cases'][:-1]),
                                dict(observed, cases=list(reversed(observed['cases']))),
                                dict(observed, upstream='wrong')]:
                    path.write_text(json.dumps(mutated))
                    with self.assertRaises(ValueError): oracle.compare()
                changed = copy.deepcopy(observed)
                changed['cases'][-1]['value']['units'][0] += 1
                path.write_text(json.dumps(changed))
                with self.assertRaises(ValueError): oracle.compare()
    def test_duplicate_and_unknown_fields(self):
        with self.assertRaises(ValueError):
            json.loads('{"schema":1,"schema":1}', object_pairs_hook=oracle.unique)
        with self.assertRaises(ValueError):
            oracle.value({'tag':'f64','bits':'3ff0000000000000','extra':False})
    def test_invalid_bits_and_boolean(self):
        for node in [{'tag':'f64','bits':'1'}, {'tag':'bool','value':1}, {'tag':'vector','items':[{}]}]:
            with self.assertRaises(ValueError): oracle.value(node)
    def test_error_ids_and_messages_strict(self):
        original = json.loads((oracle.ROOT/'tests/oracle/subvector-errors.json').read_text())
        for changed in [original+[original[0]], [dict(original[0], message=None)]]:
            with patch('subvector_oracle.json.loads', return_value=changed):
                with self.assertRaises(ValueError): oracle.errors()

if __name__ == '__main__': unittest.main()
