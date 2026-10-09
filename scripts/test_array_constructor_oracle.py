"""Reject incomplete or mistyped reference observations."""
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import array_constructor_oracle as oracle

class ArrayConstructorOracleTests(unittest.TestCase):
    def test_closed_ordered_observations_fail_closed(self):
        entries = oracle.cases()
        expected = [{'id': c['id'], 'value': True} for c in entries]
        malformed = [expected[:-1], expected[::-1], expected + [expected[0]],
                     [{'id': 'unknown', 'value': True}] + expected[1:],
                     [{'id': expected[0]['id'], 'value': 1}] + expected[1:],
                     [{'id': expected[0]['id'], 'value': True, 'extra': 1}] + expected[1:]]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            target = root / 'tests/oracle/out/array-constructor-observations.json'
            target.parent.mkdir(parents=True)
            with patch.object(oracle, 'ROOT', root), patch.object(oracle, 'cases', return_value=entries):
                for rows in malformed:
                    with self.subTest(rows=rows):
                        target.write_text(json.dumps(rows))
                        with self.assertRaises(ValueError):
                            oracle.compare()
                target.write_text(json.dumps(expected))
                oracle.compare()

    def test_substituted_or_reordered_corpus_rejected(self):
        original = oracle.read(oracle.ROOT / 'tests/oracle/array-constructor-cases.json')
        for mode in ['substitute', 'reorder']:
            changed = json.loads(json.dumps(original))
            if mode == 'substitute':
                changed['cases'][0]['id'] = 'substituted'
            else:
                changed['cases'].reverse()
            with self.subTest(mode=mode), patch.object(oracle, 'read', return_value=changed):
                with self.assertRaises(ValueError):
                    oracle.cases()

    def test_duplicate_json_keys_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'observations.json'
            path.write_text('{"value":true,"value":false}')
            with self.assertRaises(ValueError):
                oracle.read(path)

if __name__ == '__main__':
    unittest.main()
