import copy
import unittest
from record_helper_oracle import CASE_IDS, ROOT, read, cases, compare

class RecordHelperOracle(unittest.TestCase):
    def setUp(self):
        self.corpus = read(ROOT / 'tests/oracle/record-helper-cases.json')

    def test_complete_closed_source_cases(self):
        self.assertEqual(len(cases(self.corpus)), 33)

    def test_missing_reordered_duplicate_or_unknown_ids_rejected(self):
        for mutation in ('missing', 'reordered', 'duplicate', 'unknown'):
            corpus = copy.deepcopy(self.corpus)
            if mutation == 'missing': corpus['cases'].pop()
            elif mutation == 'reordered': corpus['cases'].reverse()
            elif mutation == 'duplicate': corpus['cases'][1] = copy.deepcopy(corpus['cases'][0])
            else: corpus['cases'][0]['id'] = 'unknown'
            with self.subTest(mutation=mutation), self.assertRaises(ValueError): cases(corpus)

    def test_numbers_cannot_impersonate_boolean_evidence(self):
        actual = [{'id': identity, 'value': True} for identity in CASE_IDS]
        compare(actual)
        for bad in (1, 1.0, None, 'true'):
            altered = copy.deepcopy(actual); altered[0]['value'] = bad
            with self.subTest(bad=bad), self.assertRaises(ValueError): compare(altered)
        corpus = copy.deepcopy(self.corpus); corpus['cases'][0]['expected']['value'] = 1
        with self.assertRaises(ValueError): cases(corpus)
