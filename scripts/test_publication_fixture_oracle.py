import copy
import unittest
from unittest.mock import patch
import publication_fixture_oracle as gate

class ClosedPublicationFixtureTests(unittest.TestCase):
    def test_missing_reordered_duplicate_and_unknown_ids_rejected(self):
        original = gate.corpus()
        for mutation in ('missing', 'reordered', 'duplicate', 'unknown'):
            corpus = copy.deepcopy(original)
            if mutation == 'missing': corpus['cases'].pop()
            elif mutation == 'reordered': corpus['cases'].reverse()
            elif mutation == 'duplicate': corpus['cases'][1] = copy.deepcopy(corpus['cases'][0])
            else: corpus['cases'][0]['id'] = 'unreviewed'
            with patch.object(gate.oracle, 'load', return_value=corpus):
                with self.assertRaises(ValueError): gate.corpus()

    def test_scoped_paths_restore_success_and_failure(self):
        original = gate.oracle.CORPUS, gate.oracle.OBSERVATIONS
        with gate.paths():
            self.assertEqual(gate.oracle.CORPUS, gate.CORPUS)
        self.assertEqual((gate.oracle.CORPUS, gate.oracle.OBSERVATIONS), original)
        with self.assertRaises(RuntimeError):
            with gate.paths(): raise RuntimeError('diagnostic')
        self.assertEqual((gate.oracle.CORPUS, gate.oracle.OBSERVATIONS), original)

    def test_complete_closed_forms(self):
        self.assertEqual(len(gate.corpus()['cases']), 4)

if __name__ == '__main__': unittest.main()
