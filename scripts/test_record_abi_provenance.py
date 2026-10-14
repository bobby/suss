import copy
import hashlib
import json
import unittest

from check_record_macro_provenance import ROOT, verify_named_property_abi_adaptation


class RecordAbiProvenance(unittest.TestCase):
    def setUp(self):
        self.data = json.loads(
            (ROOT / 'docs/compatibility/record-macro-foundations/abi-adaptations.json').read_text()
        )

    def test_exact_source_and_instruction_provenance(self):
        self.assertEqual(verify_named_property_abi_adaptation(self.data), 1)

    def test_stale_source_hash_rejected(self):
        changed = copy.deepcopy(self.data)
        changed['source-file-sha256'] = '0' * 64
        with self.assertRaises(ValueError):
            verify_named_property_abi_adaptation(changed)

    def test_changed_instruction_sequence_rejected(self):
        changed = copy.deepcopy(self.data)
        changed['instruction-sequence'].reverse()
        with self.assertRaises(ValueError):
            verify_named_property_abi_adaptation(changed)

    def test_mutated_guard_block_rejected_after_refreshing_file_hash(self):
        path = ROOT / self.data['source']
        source = path.read_bytes()
        old = b'I32Const(low as i32),\n            I32Sub,\n            I32Const((high - low) as i32),'
        new = b'I32Const(low as i32),\n            I32GeU,\n            I32Const((high - low) as i32),'
        self.assertIn(old, source)
        mutated = source.replace(old, new, 1) + b'\n// Decoy tokens: I32Sub I32LeU high - low\n'
        changed = copy.deepcopy(self.data)
        changed['source-file-sha256'] = hashlib.sha256(mutated).hexdigest()
        with self.assertRaises(ValueError):
            verify_named_property_abi_adaptation(changed, mutated)


if __name__ == '__main__':
    unittest.main()
