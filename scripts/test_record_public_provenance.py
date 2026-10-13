import copy
import json
import unittest
from check_record_macro_provenance import ROOT, verify_public_adaptations

class PublicRecordProvenance(unittest.TestCase):
    def setUp(self):
        self.data = json.loads((ROOT / 'docs/compatibility/record-macro-foundations/public-adaptations.json').read_text())

    def test_complete_integrated_sources(self):
        self.assertEqual(verify_public_adaptations(self.data), 3)

    def test_stale_sources_constants_and_inputs_rejected(self):
        for kind in ['source', 'constant', 'input']:
            with self.subTest(kind=kind):
                data = copy.deepcopy(self.data)
                if kind == 'source': data['sources'][0]['sha256'] = '0' * 64
                elif kind == 'constant': data['sources'][0]['compiled-sources']['SOURCE'] = '0' * 64
                else: data['retained-inputs'][0]['sha256'] = '0' * 64
                with self.assertRaises(ValueError): verify_public_adaptations(data)

    def test_missing_reordered_and_duplicate_identities_rejected(self):
        for key in ['sources', 'retained-inputs']:
            for kind in ['missing', 'reordered', 'duplicate']:
                with self.subTest(key=key, kind=kind):
                    data = copy.deepcopy(self.data)
                    if kind == 'missing': data[key].pop()
                    elif kind == 'reordered': data[key].reverse()
                    else: data[key][1] = copy.deepcopy(data[key][0])
                    with self.assertRaises(ValueError): verify_public_adaptations(data)

if __name__ == '__main__': unittest.main()
