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
            'checked-validation-sha256': '0' * 64,
            'short-route-sha256': '0' * 64,
            'short-precondition': 'unchecked cached slot',
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
        for route in ['generic', 'checked-validation']:
            route_mutations = mutations.copy()
            if route == 'checked-validation':
                # Character-template lines move inside a closure; reserved and
                # equality generator lines keep their original indentation.
                for key in ['non_ascii_comparison', 'mask_word_selection',
                            'mask_bit_test', 'lowercase_bypass_reserved']:
                    old, new = route_mutations[key]
                    route_mutations[key] = (old.replace(b'\n', b'\n    '),
                                            new.replace(b'\n', b'\n    '))
                # Same twelve algorithm boundaries in the fused generator;
                # successful validation exits a block rather than returning true.
                route_mutations['reserved_rejection'] = (
                    b'                nominal::error(&mut body);',
                    b'                body.extend([I32Const(0), Return]);')
                route_mutations['reserved_length_bucket_acceptance'] = (
                    b'body.extend([Br(1), End]);', b'body.extend([Br(0), End]);')
                route_mutations['empty_name_acceptance'] = (
                    b'LocalGet(5),\n        I32Eqz,\n        If(BlockType::Empty),\n        Br(1),',
                    b'LocalGet(5),\n        I32Eqz,\n        If(BlockType::Empty),\n        Br(0),')
            for name, (old, new) in route_mutations.items():
                with self.subTest(route=route, mutation=name):
                    scope = scopes[route]
                    self.assertEqual(scope.count(old), 1)
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
            (b'Call(field_match_checked),', b'I32Const(1),'),
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

    def test_combined_comparator_and_throw_cannot_be_bypassed(self):
        source = (ROOT / self.data['source']).read_bytes()
        scopes = named_property_guard_scopes(source)
        for route, field, old, new in [
            ('checked', 'checked-guard-sha256',
             b'body.extend([LocalGet(5), LocalGet(2)]);',
             b'body.extend([LocalGet(5), LocalGet(5)]);'),
            ('checked', 'checked-guard-sha256',
             b'            ArrayGetU(STRING),', b'            ArrayLen,'),
            ('checked-validation', 'checked-validation-sha256',
             b'    nominal::error(&mut body);', b'    body.extend([I32Const(0), Return]);'),
            ('checked-validation', 'checked-validation-sha256',
             b'    body.push(End); // successful validation exits this block, never the function.',
             b'    body.extend([End, I32Const(1), Return]);'),
        ]:
            with self.subTest(route=route, bypass=old):
                scope = scopes[route]
                self.assertIn(old, scope)
                mutated_scope = scope.replace(old, new, 1)
                mutated = source.replace(scope, mutated_scope, 1)
                changed = copy.deepcopy(self.data)
                changed['source-file-sha256'] = hashlib.sha256(mutated).hexdigest()
                with self.assertRaisesRegex(ValueError, 'complete guard differs'):
                    verify_named_property_abi_adaptation(changed, mutated)
                changed[field] = hashlib.sha256(mutated_scope).hexdigest()
                with self.assertRaisesRegex(ValueError, 'guard identity mismatch'):
                    verify_named_property_abi_adaptation(changed, mutated)

    def test_checked_prefix_cannot_skip_unit_or_reserved_validation(self):
        source = (ROOT / self.data['source']).read_bytes()
        checked = named_property_guard_scopes(source)['checked-validation']
        for old, new in [
            (b'    body.push(Block(BlockType::Empty));\n    for index in 0..4 {',
             b'    body.push(Block(BlockType::Empty));\n    for index in 0..3 {'),
            (b'If(BlockType::Empty), Br(1), End,',
             b'If(BlockType::Empty), I32Const(1), Return, End,'),
            (b'I32Const(4), LocalSet(3), Block(BlockType::Empty), Loop(BlockType::Empty),',
             b'I32Const(5), LocalSet(3), Block(BlockType::Empty), Loop(BlockType::Empty),'),
            (b'        emit_unit_guard(&mut body);', b'        // prefix unit check removed'),
            (b'\n    emit_unit_guard(&mut body);\n', b'\n    // fallback unit check removed\n'),
            (b'LocalGet(3), LocalGet(5), I32GeU, BrIf(1),',
             b'LocalGet(3), LocalGet(5), I32GtU, BrIf(1),'),
        ]:
            with self.subTest(bypass=old):
                self.assertIn(old, checked)
                changed_scope = checked.replace(old, new, 1)
                mutated = source.replace(checked, changed_scope, 1)
                mutated += b'\n/* Decoy: ' + old + b' */\n'
                changed = copy.deepcopy(self.data)
                changed['source-file-sha256'] = hashlib.sha256(mutated).hexdigest()
                with self.assertRaisesRegex(ValueError, 'complete guard differs'):
                    verify_named_property_abi_adaptation(changed, mutated)
                changed['checked-validation-sha256'] = hashlib.sha256(changed_scope).hexdigest()
                with self.assertRaisesRegex(ValueError, 'guard identity mismatch'):
                    verify_named_property_abi_adaptation(changed, mutated)

    def test_refreshed_guard_hash_cannot_authorize_new_algorithm(self):
        source = (ROOT / self.data['source']).read_bytes()
        scopes = named_property_guard_scopes(source)
        for route, field in [('generic', 'guard-sha256'),
                             ('checked-validation', 'checked-validation-sha256')]:
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

    def test_shared_validator_calls_and_packed_comparison_are_closed(self):
        source = (ROOT / self.data['source']).read_bytes()
        scope = named_property_guard_scopes(source)['checked']
        for old, new in [
            (b'let mut body = checked_field_validation();', b'let mut body = vec![];'),
            (b'let mut short_body = checked_field_validation();', b'let mut short_body = vec![];'),
            (b'I32Const(0xffff), I32And, I32Ne,', b'I32Const(0x7fff), I32And, I32Ne,'),
        ]:
            with self.subTest(bypass=old):
                self.assertEqual(scope.count(old), 1)
                changed_scope = scope.replace(old, new, 1)
                mutated = source.replace(scope, changed_scope, 1)
                changed = copy.deepcopy(self.data)
                changed['source-file-sha256'] = hashlib.sha256(mutated).hexdigest()
                with self.assertRaisesRegex(ValueError, 'complete guard differs'):
                    verify_named_property_abi_adaptation(changed, mutated)
                changed['checked-guard-sha256'] = hashlib.sha256(changed_scope).hexdigest()
                with self.assertRaisesRegex(ValueError, 'guard identity mismatch'):
                    verify_named_property_abi_adaptation(changed, mutated)

    def test_short_route_guards_scan_and_fallback_are_closed(self):
        source = (ROOT / self.data['source']).read_bytes()
        scope = named_property_guard_scopes(source)['short-route']
        for old, new in [
            (b'I32Const(4), I32GtU,', b'I32Const(5), I32GtU,'),
            (b'guard(&mut body,3,ARGS);', b'// payload guard removed'),
            (b'guard(&mut body,4,ARGS);', b'// schema guard removed'),
            (b'guard(&mut body,5,STRING);', b'// entry guard removed'),
            (b'Call(field_match),', b'I32Const(1),'),
            (b'LocalGet(6),LocalSet(7),', b'LocalGet(6),Drop,'),
            (b'Br(0),End,End,', b'Br(1),End,End,'),
            (b'I32Const(0xffff),I32And,', b'I32Const(0x7fff),I32And,'),
            (b'Call(b.names["named-property-get"])', b'Call(b.names["nil"])'),
            (b'    body.push(I32Const(1));', b'    body.push(I32Const(0));'),
            (b'body.extend([I32Const(0),I32Const(0),RefI31]);',
             b'body.extend([I32Const(1),I32Const(0),RefI31]);'),
            (b'&[ValType::I32,VALUE],', b'&[VALUE],'),
        ]:
            with self.subTest(bypass=old):
                self.assertEqual(scope.count(old), 1)
                changed_scope = scope.replace(old, new, 1)
                mutated = source.replace(scope, changed_scope, 1)
                changed = copy.deepcopy(self.data)
                changed['source-file-sha256'] = hashlib.sha256(mutated).hexdigest()
                with self.assertRaisesRegex(ValueError, 'complete guard differs'):
                    verify_named_property_abi_adaptation(changed, mutated)
                changed['short-route-sha256'] = hashlib.sha256(changed_scope).hexdigest()
                with self.assertRaisesRegex(ValueError, 'guard identity mismatch'):
                    verify_named_property_abi_adaptation(changed, mutated)



if __name__ == '__main__':
    unittest.main()
