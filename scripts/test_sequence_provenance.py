import copy
import unittest
from unittest.mock import patch
import sequence_provenance as provenance


class SequenceProvenance(unittest.TestCase):
    def test_printer_expansion_rejects_rewritten_reordered_and_invented_stanzas(self):
        statement = next(s for s in self.record['statements']
                         if s.get('patch') == 'docs/compatibility/patches/printing-collection-extensions.json')
        original = provenance.json_data(
            provenance.repository_file(provenance.ROOT, statement['patch']).read_bytes())
        forms = provenance.Scanner(original['replacement']).all()[0].children[1:]
        first, second = [original['replacement'][f.start:f.end] for f in forms[:2]]
        for replacement in [
            original['replacement'].replace('LazySeq', 'InventedSeq', 1),
            original['replacement'].replace('opts coll)', 'opts nil)', 1),
            '(do ' + second + first + ')',
            '(do ' + first + first + ')',
            '(do)',
        ]:
            changed = copy.deepcopy(original)
            changed['replacement'] = replacement
            decode = provenance.json_data
            def changed_patch(raw):
                value = decode(raw)
                return changed if value == original else value
            with patch.object(provenance, 'json_data', side_effect=changed_patch):
                with self.assertRaisesRegex(ValueError, 'ordered exact source stanzas'):
                    provenance.verify_payload(self.record, self.source, self.loader)

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

    def test_vector_property_munging_preserves_exact_canonical_target(self):
        statement = next(s for s in self.record['statements']
                         if s.get('patch') == 'docs/compatibility/patches/vector-empty-node.json')
        original = provenance.json_data(
            provenance.repository_file(provenance.ROOT, statement['patch']).read_bytes())
        changed = copy.deepcopy(original)
        changed['replacement'] = changed['replacement'].replace('EMPTY_NODE', 'DIFFERENT_NODE')
        decode = provenance.json_data
        def changed_patch(raw):
            value = decode(raw)
            return changed if value == original else value
        with patch.object(provenance, 'json_data', side_effect=changed_patch):
            with self.assertRaisesRegex(ValueError, 'preserve one complete form and target'):
                provenance.verify_payload(self.record, self.source, self.loader)

    def test_map_threshold_munging_preserves_exact_canonical_target(self):
        statement = next(s for s in self.record['statements']
                         if s.get('patch') == 'docs/compatibility/patches/array-map-threshold.json')
        original = provenance.json_data(
            provenance.repository_file(provenance.ROOT, statement['patch']).read_bytes())
        changed = copy.deepcopy(original)
        changed['replacement'] = changed['replacement'].replace('HASHMAP_THRESHOLD', 'OTHER_THRESHOLD')
        decode = provenance.json_data
        def changed_patch(raw):
            value = decode(raw)
            return changed if value == original else value
        with patch.object(provenance, 'json_data', side_effect=changed_patch):
            with self.assertRaisesRegex(ValueError, 'preserve one complete form and target'):
                provenance.verify_payload(self.record, self.source, self.loader)
