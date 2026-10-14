"""Reject incomplete or case-colliding retained source evidence."""
import copy
import unittest
from check_transducer_provenance import IDS, PIN, verify_retention
from core_import import digest

class RetentionTests(unittest.TestCase):
    def setUp(self):
        self.originals = {identity: {'form': f'(def declaration-{i} {i})'} for i, identity in enumerate(IDS)}
        self.files = {f'{i}-source.cljs': (self.originals[identity]['form'] + '\n').encode()
                      for i, identity in enumerate(IDS)}
        self.ledger = {'upstream': PIN, 'license': 'EPL-1.0', 'forms': [
            {'id': identity, 'retained': f'{i}-source.cljs',
             'sha256': digest(self.originals[identity]['form'].encode()),
             'retained-file-sha256': digest(self.files[f'{i}-source.cljs'])}
            for i, identity in enumerate(IDS)]}

    def test_complete_retention(self):
        verify_retention(self.ledger, self.originals, self.files)

    def test_changed_license_is_rejected(self):
        self.ledger['license'] = 'MIT'
        with self.assertRaisesRegex(ValueError, 'license'):
            verify_retention(self.ledger, self.originals, self.files)

    def test_missing_declaration_is_not_an_empty_success(self):
        self.ledger['forms'].pop()
        with self.assertRaisesRegex(ValueError, 'identities'):
            verify_retention(self.ledger, self.originals, self.files)

    def test_duplicate_identity_is_rejected(self):
        self.ledger['forms'][1] = copy.deepcopy(self.ledger['forms'][0])
        with self.assertRaisesRegex(ValueError, 'identities'):
            verify_retention(self.ledger, self.originals, self.files)

    def test_case_collision_is_rejected_before_reading_files(self):
        self.ledger['forms'][3]['retained'] = 'Empty.cljs'
        self.ledger['forms'][4]['retained'] = 'EMPTY.cljs'
        with self.assertRaisesRegex(ValueError, 'collide'):
            verify_retention(self.ledger, self.originals, self.files)

    def test_changed_source_cannot_be_blessed_by_updating_only_retained_hash(self):
        self.files['0-source.cljs'] = b'(def different 99)\n'
        self.ledger['forms'][0]['retained-file-sha256'] = digest(self.files['0-source.cljs'])
        with self.assertRaisesRegex(ValueError, 'whole original'):
            verify_retention(self.ledger, self.originals, self.files)

    def test_stale_retained_hash_is_rejected(self):
        self.ledger['forms'][0]['retained-file-sha256'] = '0' * 64
        with self.assertRaisesRegex(ValueError, 'retained file hash'):
            verify_retention(self.ledger, self.originals, self.files)

if __name__ == '__main__':
    unittest.main()
