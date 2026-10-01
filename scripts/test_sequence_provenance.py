import copy
import unittest
from unittest.mock import patch
import sequence_provenance as provenance


class SequenceProvenance(unittest.TestCase):
    def setUp(self):
        self.record = provenance.json_data((provenance.ROOT / provenance.PROVENANCE).read_bytes())
        self.source = (provenance.ROOT / 'clojurescript/src/main/cljs/cljs/core.cljs').read_text()
        self.loader = (provenance.ROOT / self.record['loader']).read_text()

    def test_complete_source_forms_and_notice_match(self):
        provenance.verify_payload(self.record, self.source, self.loader)

    def test_changed_bounds_hash_body_and_extra_setup_are_rejected(self):
        for field, value in [('line', self.record['statements'][0]['line'] + 1),
                             ('sha256', '0' * 64)]:
            record = copy.deepcopy(self.record)
            record['statements'][0][field] = value
            with self.assertRaisesRegex(ValueError, 'complete matching source form'):
                provenance.verify_payload(record, self.source, self.loader)
        for loader in [self.loader.replace('(EmptyList. nil)', '(EmptyList. false)'),
                       self.loader + '\n(def unreviewed 7)\n']:
            with self.assertRaisesRegex(ValueError, 'differs from retained'):
                provenance.verify_payload(self.record, self.source, loader)

    def test_missing_license_notice_is_rejected(self):
        with self.assertRaisesRegex(ValueError, 'complete upstream notice'):
            provenance.verify_payload(self.record, self.source, self.loader[self.loader.index('(set!'):])

    def test_standalone_patch_rejects_stale_hash_target_and_extra_forms(self):
        statement = next(s for s in self.record['statements'] if 'patch' in s)
        original = provenance.json_data(
            provenance.repository_file(provenance.ROOT, statement['patch']).read_bytes())
        for field, value in [
            ('source-sha256', '0' * 64),
            ('replacement', '(extend-type number IHash (-hash [o] 0))'),
            ('replacement', original['replacement'] + '\n(def extra 7)'),
            ('rationale', ''),
            ('schema', True),
        ]:
            changed = copy.deepcopy(original)
            changed[field] = value
            with patch.object(provenance, 'json_data', return_value=changed):
                with self.assertRaises(ValueError):
                    provenance.verify_payload(self.record, self.source, self.loader)

        changed_record = copy.deepcopy(self.record)
        next(s for s in changed_record['statements'] if 'patch' in s)['patch-sha256'] = '0' * 64
        with self.assertRaisesRegex(ValueError, 'patch file hash mismatch'):
            provenance.verify_payload(changed_record, self.source, self.loader)
