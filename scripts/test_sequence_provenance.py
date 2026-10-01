import copy
import unittest
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
