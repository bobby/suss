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

    def test_changed_manifest_contract_rejected(self):
        for key, value in {
            'upstream': '0' * 40,
            'allowed-ascii': self.data['allowed-ascii'] + '@',
            'algorithm': 'range-loop',
            'guard-sha256': '0' * 64,
            'guard-location': 'decoy',
        }.items():
            with self.subTest(key=key):
                changed = copy.deepcopy(self.data)
                changed[key] = value
                with self.assertRaises(ValueError):
                    verify_named_property_abi_adaptation(changed)

    def test_mutated_guard_rejected_after_refreshing_file_hash(self):
        source = (ROOT / self.data['source']).read_bytes()
        mutations = {
            'non_ascii_boundary': (b'I32Const(128),', b'I32Const(256),'),
            'non_ascii_comparison': (
                b'I32Const(128),\n        I32GeU,',
                b'I32Const(128),\n        I32GtU,'),
            'mask_whitelist': (
                b'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_$',
                b'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_$@'),
            'mask_word_selection': (b'I32Const(64),\n        I32LtU,',
                                    b'I32Const(64),\n        I32GtU,'),
            'mask_bit_test': (b'I64And,\n        I64Eqz,', b'I64Or,\n        I64Eqz,'),
            'lowercase_bound': (b'I32Const(25),', b'I32Const(26),'),
            'lowercase_bypass_reserved': (
                b'I32Const(25),\n        I32LeU,\n        If(BlockType::Empty),',
                b'I32Const(25),\n        I32LeU,\n        If(BlockType::Empty), I32Const(1), Return,'),
            'reserved_spelling': (b'        "constructor",\n        "__proto__",',
                                  b'        "constructoz",\n        "__proto__",'),
            'reserved_tail_comparison': (b'I32Eq,\n                        I32And,',
                                         b'I32Ne,\n                        I32And,'),
            'reserved_rejection': (
                b'body.extend([If(BlockType::Empty), I32Const(0), Return, End]);',
                b'body.extend([If(BlockType::Empty), I32Const(1), Return, End]);'),
            'reserved_length_bucket_acceptance': (
                b'body.extend([I32Const(1), Return, End]);',
                b'body.extend([I32Const(0), Return, End]);'),
            'empty_name_acceptance': (
                b'LocalGet(3),\n        I32Eqz,\n        If(BlockType::Empty),\n        I32Const(1),',
                b'LocalGet(3),\n        I32Eqz,\n        If(BlockType::Empty),\n        I32Const(0),'),
        }
        for name, (old, new) in mutations.items():
            with self.subTest(mutation=name):
                self.assertEqual(source.count(old), 1)
                # Leave the original tokens as a decoy outside the guard.
                mutated = source.replace(old, new, 1) + b'\n/* Decoy: ' + old + b' */\n'
                changed = copy.deepcopy(self.data)
                changed['source-file-sha256'] = hashlib.sha256(mutated).hexdigest()
                with self.assertRaisesRegex(ValueError, 'complete guard differs'):
                    verify_named_property_abi_adaptation(changed, mutated)

    def test_refreshed_guard_hash_cannot_authorize_new_algorithm(self):
        source = (ROOT / self.data['source']).read_bytes()
        mutated = source.replace(b'I32Const(128),', b'I32Const(256),', 1)
        start = mutated.index(b'    // Named instance access must not mistake')
        end = mutated.index(b'    // Stride one is a field schema')
        changed = copy.deepcopy(self.data)
        changed['source-file-sha256'] = hashlib.sha256(mutated).hexdigest()
        changed['guard-sha256'] = hashlib.sha256(mutated[start:end]).hexdigest()
        with self.assertRaisesRegex(ValueError, 'guard identity mismatch'):
            verify_named_property_abi_adaptation(changed, mutated)


if __name__ == '__main__':
    unittest.main()
