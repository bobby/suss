import copy
import hashlib
import json
import unittest

from check_record_macro_provenance import ROOT, verify_named_property_abi_adaptation, named_property_guard_scopes


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
        scopes = named_property_guard_scopes(source)
        for route in ['generic', 'checked']:
            for name, (old, new) in mutations.items():
                with self.subTest(route=route, mutation=name):
                    scope = scopes[route]
                    self.assertEqual(scope.count(old), 1)
                    # Mutate only this actual generator; retain the other valid
                    # route and append decoy original tokens outside both scopes.
                    mutated = source.replace(scope, scope.replace(old, new, 1), 1)
                    mutated += b'\n/* Decoy: ' + old + b' */\n'
                    changed = copy.deepcopy(self.data)
                    changed['source-file-sha256'] = hashlib.sha256(mutated).hexdigest()
                    with self.assertRaisesRegex(ValueError, 'complete guard differs'):
                        verify_named_property_abi_adaptation(changed, mutated)

    def test_checked_route_bypasses_rejected_with_refreshed_hash_and_decoys(self):
        source = (ROOT / self.data['source']).read_bytes()
        route = named_property_guard_scopes(source)['typed-route']
        for old, new in [
            (b'    guard(&mut body, 4, STRING);', b'    // entry guard removed'),
            (b'Call(field_supported_checked),', b'I32Const(1),'),
            (b'Call(equal_checked),', b'I32Const(1),'),
            (b'        Br(0),', b'        Br(1),'),
        ]:
            with self.subTest(bypass=old):
                self.assertIn(old, route)
                changed_route = route.replace(old, new, 1)
                mutated = source.replace(route, changed_route, 1) + b'\n/* Decoy: ' + old + b' */\n'
                changed = copy.deepcopy(self.data)
                changed['source-file-sha256'] = hashlib.sha256(mutated).hexdigest()
                with self.assertRaisesRegex(ValueError, 'complete guard differs'):
                    verify_named_property_abi_adaptation(changed, mutated)
                changed['route-sha256'] = hashlib.sha256(changed_route).hexdigest()
                with self.assertRaisesRegex(ValueError, 'guard identity mismatch'):
                    verify_named_property_abi_adaptation(changed, mutated)

    def test_refreshed_guard_hash_cannot_authorize_new_algorithm(self):
        source = (ROOT / self.data['source']).read_bytes()
        scopes = named_property_guard_scopes(source)
        for route, field in [('generic', 'guard-sha256'),
                             ('checked', 'checked-guard-sha256')]:
            with self.subTest(route=route):
                scope = scopes[route]
                self.assertEqual(scope.count(b'I32Const(128),'), 1)
                mutated = source.replace(scope, scope.replace(
                    b'I32Const(128),', b'I32Const(256),', 1), 1)
                changed = copy.deepcopy(self.data)
                changed['source-file-sha256'] = hashlib.sha256(mutated).hexdigest()
                actual_scope = named_property_guard_scopes(mutated)[route]
                if route == 'generic':
                    # The generic manifest binds the original canonical spelling.
                    actual_scope = actual_scope.replace(
                        b'let _field_supported =', b'let field_supported =')
                changed[field] = hashlib.sha256(actual_scope).hexdigest()
                with self.assertRaisesRegex(ValueError, 'guard identity mismatch'):
                    verify_named_property_abi_adaptation(changed, mutated)



if __name__ == '__main__':
    unittest.main()
