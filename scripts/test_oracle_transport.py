import copy
import json
import unittest
from pathlib import Path

from oracle_transport import unique, validate, value


REFERENCE = Path(__file__).resolve().parents[1] / 'tests/oracle/reference.json'


class OracleTransportTests(unittest.TestCase):
    def setUp(self):
        self.reference = json.loads(REFERENCE.read_text(), object_pairs_hook=unique)

    def test_executed_reference_snapshot(self):
        self.assertEqual(validate(self.reference), 12)

    def test_missing_duplicate_or_changed_cases_fail(self):
        for change in ('missing', 'duplicate', 'unknown-status', 'unknown-field', 'wrong-pin'):
            document = copy.deepcopy(self.reference)
            if change == 'missing':
                document['cases'].pop()
            elif change == 'duplicate':
                document['cases'][1] = document['cases'][0]
            elif change == 'unknown-status':
                document['cases'][0]['status'] = 'unknown'
            elif change == 'unknown-field':
                document['cases'][0]['passed'] = True
            else:
                document['upstream'] = 'latest'
            with self.subTest(change=change), self.assertRaises(ValueError):
                validate(document)

    def test_lossy_boundaries_and_reordered_effects_fail(self):
        for identity in ('negative-zero', 'nan', 'utf16-surrogate', 'argument-order'):
            document = copy.deepcopy(self.reference)
            case = next(c for c in document['cases'] if c['id'] == identity)
            if identity == 'argument-order':
                case['effects'].reverse()
            elif identity == 'utf16-surrogate':
                case['value']['units'] = [65533]
            else:
                case['value']['bits'] = '0000000000000000'
            with self.subTest(identity=identity), self.assertRaises(ValueError):
                validate(document)

    def test_unknown_layouts_and_malformed_nested_values_fail(self):
        invalid = [None, {'tag': 'opaque'}, {'tag': 'bool', 'value': 1},
                   {'tag': 'f64', 'bits': 'NaN'}, {'tag': 'string', 'units': [True]},
                   {'tag': 'string', 'units': [65536]}, {'tag': 'map', 'entries': [[{'tag': 'nil'}]]},
                   {'tag': 'vector', 'items': [{'tag': 'opaque'}]},
                   {'tag': 'symbol', 'namespace': {'tag': 'nil'}, 'name': {'tag': 'nil'}}]
        for node in invalid:
            with self.subTest(node=node), self.assertRaises(ValueError):
                value(node)

    def test_effects_and_exception_messages_cannot_be_other_tagged_values(self):
        for field in ('effects', 'message'):
            document = copy.deepcopy(self.reference)
            case = document['cases'][-1]
            case[field] = [{'tag': 'nil'}] if field == 'effects' else {'tag': 'bool', 'value': True}
            with self.subTest(field=field), self.assertRaises(ValueError):
                validate(document)

    def test_serializer_exception_cannot_replace_required_value(self):
        document = copy.deepcopy(self.reference)
        document['cases'][8] = dict(document['cases'][-1], id='nested-values', effects=[])
        with self.assertRaises(ValueError):
            validate(document)

    def test_duplicate_json_keys_and_trailing_data_fail(self):
        for source in ('{"schema":1,"schema":2}', '{"tag":"nil"} {}'):
            with self.subTest(source=source), self.assertRaises(ValueError):
                json.loads(source, object_pairs_hook=unique)


if __name__ == '__main__':
    unittest.main()
