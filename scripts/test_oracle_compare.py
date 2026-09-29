import copy
import json
import unittest
from pathlib import Path
from oracle_cases import load
from oracle_compare import baseline, compare, matches
from oracle_transport import unique

ROOT = Path(__file__).resolve().parents[1] / 'tests/oracle'


class DifferentialTests(unittest.TestCase):
    def setUp(self):
        self.reference = json.loads((ROOT / 'reference.json').read_text(), object_pairs_hook=unique)

    def test_shared_corpus_and_reference_ids_match(self):
        cases = load((ROOT / 'cases.json').read_text())
        self.assertEqual([c['id'] for c in cases], [c['id'] for c in self.reference['cases']])
        self.assertEqual(compare(self.reference, self.reference), {})

    def test_missing_and_malformed_corpus_cannot_be_empty_success(self):
        corpus = json.loads((ROOT / 'cases.json').read_text())
        for alteration in ('missing', 'duplicate', 'blank', 'unknown-field'):
            document = copy.deepcopy(corpus)
            if alteration == 'missing':
                document['cases'] = []
            elif alteration == 'duplicate':
                document['cases'][1] = document['cases'][0]
            elif alteration == 'blank':
                document['cases'][0]['expr'] = ' '
            else:
                document['cases'][0]['skip'] = True
            with self.subTest(alteration=alteration), self.assertRaises(ValueError):
                load(json.dumps(document))

    def test_failures_preserve_stage_diagnostic_and_unexpected_pass(self):
        observed = copy.deepcopy(self.reference)
        observed['cases'][0] = {'id': 'condition-once', 'status': 'failure', 'stage': 'compile', 'diagnostic': 'unbound var'}
        failures = compare(self.reference, observed)
        expected = {'condition-once': {'stage': 'compile', 'diagnostic': 'unbound var'}}
        self.assertEqual(failures, baseline(json.dumps(expected)))
        observed['cases'][0]['diagnostic'] = 'changed'
        self.assertNotEqual(compare(self.reference, observed), expected)
        observed['cases'][0]['stage'] = 'decode'
        self.assertNotEqual(compare(self.reference, observed), expected)
        self.assertNotEqual(compare(self.reference, self.reference), expected)

    def test_values_effects_and_status_are_compared(self):
        for change in ('bits', 'effects', 'exception'):
            observed = copy.deepcopy(self.reference)
            if change == 'bits':
                observed['cases'][3]['value']['bits'] = '0000000000000000'
            elif change == 'effects':
                observed['cases'][1]['effects'].reverse()
            else:
                observed['cases'][-1] = dict(observed['cases'][0], id=observed['cases'][-1]['id'])
            failures = compare(self.reference, observed)
            self.assertEqual(len(failures), 1)
            self.assertEqual(next(iter(failures.values()))['stage'], 'value')

    def test_maps_and_sets_ignore_order_but_sequences_and_kinds_do_not(self):
        nested = self.reference['cases'][8]['value']
        reverse = copy.deepcopy(nested)
        reverse['entries'].reverse()
        reverse['entries'][1][1]['items'].reverse()
        self.assertTrue(matches(nested, reverse))
        sequence = nested['entries'][2][1]
        changed = copy.deepcopy(sequence)
        changed['items'].reverse()
        self.assertFalse(matches(sequence, changed))
        changed = dict(sequence, tag='vector')
        self.assertFalse(matches(sequence, changed))
        duplicate = copy.deepcopy(nested['entries'][1][1])
        duplicate['items'][1] = duplicate['items'][0]
        self.assertFalse(matches(nested['entries'][1][1], duplicate))

    def test_missing_unknown_or_opaque_observations_fail(self):
        for change in ('missing', 'opaque', 'unknown', 'blank-diagnostic'):
            observed = copy.deepcopy(self.reference)
            if change == 'missing':
                observed['cases'].pop()
            elif change == 'opaque':
                observed['cases'][0]['value'] = {'tag': 'opaque'}
            else:
                observed['cases'][0] = {'id': 'condition-once', 'status': 'failure', 'stage': 'unknown' if change == 'unknown' else 'decode', 'diagnostic': 'error' if change == 'unknown' else ''}
            with self.subTest(change=change), self.assertRaises(ValueError):
                compare(self.reference, observed)

    def test_malformed_failure_baselines_fail(self):
        invalid = [[], {'unknown-case': {'stage': 'compile', 'diagnostic': 'error'}},
                   {'condition-once': {'stage': 'compile', 'diagnostic': ''}},
                   {'condition-once': {'stage': 'value', 'actual': {}}},
                   {'condition-once': {'stage': 'value', 'expected': {}, 'actual': {}}}]
        for document in invalid:
            with self.subTest(document=document), self.assertRaises(ValueError):
                baseline(json.dumps(document))
        with self.assertRaises(ValueError):
            baseline('{"condition-once":{},"condition-once":{}}')

    def test_thrown_values_cannot_be_discarded_or_changed(self):
        observed = copy.deepcopy(self.reference)
        case = next(c for c in observed['cases'] if c['id'] == 'throw-string')
        case['thrown'] = {'tag': 'nil'}
        self.assertEqual(compare(self.reference, observed)['throw-string']['stage'], 'value')
        del case['thrown']
        with self.assertRaises(ValueError):
            compare(self.reference, observed)


if __name__ == '__main__':
    unittest.main()
