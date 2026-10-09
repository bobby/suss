import copy
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
import portable_oracle
import record_helper_raw_oracle as raw

class RawHelperOracle(unittest.TestCase):
    def setUp(self):
        self.original = json.loads(raw.CORPUS.read_text())

    def load(self, corpus):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'corpus.json'
            path.write_text(json.dumps(corpus))
            with patch.object(raw, 'CORPUS', path):
                return raw.corpus()

    def test_closed_ids_and_one_form(self):
        self.assertEqual(len(raw.corpus()['cases']), 33)
        for change in ('missing', 'reordered', 'duplicate', 'unknown', 'extra-form'):
            corpus = copy.deepcopy(self.original)
            if change == 'missing': corpus['cases'].pop()
            elif change == 'reordered': corpus['cases'].reverse()
            elif change == 'duplicate': corpus['cases'][1] = copy.deepcopy(corpus['cases'][0])
            elif change == 'unknown': corpus['cases'][0]['id'] = 'unknown'
            else: corpus['cases'][0]['source'] += ' nil'
            with self.subTest(change=change), self.assertRaises(ValueError): self.load(corpus)

    def test_paths_restored_on_failure_and_success(self):
        saved = portable_oracle.CORPUS, portable_oracle.OBSERVATIONS
        raw.corpus()
        self.assertEqual(saved, (portable_oracle.CORPUS, portable_oracle.OBSERVATIONS))
        with patch.object(raw, 'CORPUS', Path('/missing-helper-corpus')):
            with self.assertRaises(FileNotFoundError): raw.corpus()
        self.assertEqual(saved, (portable_oracle.CORPUS, portable_oracle.OBSERVATIONS))

    def test_raw_transport_rejects_bool_number_and_wrong_bits(self):
        expected = {'schema':1, 'upstream':self.original['upstream'], 'cases':[
            {'id':case['id'], 'value':case['expected']} for case in self.original['cases']]}
        for change in ('bool-number', 'signed-zero', 'trace-order', 'unknown-tag'):
            actual = copy.deepcopy(expected)
            if change == 'bool-number': actual['cases'][0]['value']['items'][0]['value'] = 0
            elif change == 'signed-zero': actual['cases'][31]['value']['items'][1]['items'][0]['items'][0]['bits'] = '8000000000000000'
            elif change == 'trace-order': actual['cases'][18]['value']['items'][1]['items'].reverse()
            else: actual['cases'][0]['value']['tag'] = 'wildcard'
            with tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / 'observations.json'; path.write_text(json.dumps(actual))
                with patch.object(raw, 'OBSERVATIONS', path), self.subTest(change=change), self.assertRaises(ValueError): raw.compare()
