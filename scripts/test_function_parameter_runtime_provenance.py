import copy
import json
import unittest
from check_function_parameter_runtime_provenance import ROOT, IDS, verify

class ParameterRuntimeProvenance(unittest.TestCase):
    def setUp(self):
        self.recipe = json.loads((ROOT / 'docs/compatibility/core-import.json').read_text())
        self.patches = {entry['id']: json.loads((ROOT / entry['patch']).read_text()) for entry in self.recipe['forms'][350:356]}
    def test_complete_group(self):
        self.assertEqual(verify(self.recipe, self.patches), 6)
    def test_missing_reordered_duplicate_and_wrong_hash(self):
        for change in ('missing', 'reorder', 'duplicate', 'hash'):
            data = copy.deepcopy(self.recipe)
            if change == 'missing': data['forms'].pop(355)
            elif change == 'reorder': data['forms'][350:356] = data['forms'][350:356][::-1]
            elif change == 'duplicate': data['forms'].append(copy.deepcopy(data['forms'][350]))
            else: data['forms'][350]['source-sha256'] = '0' * 64
            with self.subTest(change=change), self.assertRaises(ValueError): verify(data, self.patches)
    def test_omitted_branch_booleanization_or_alias_rejected(self):
        for identity, source in [(IDS[0], '(def LITE_MODE true)'), (IDS[2], '(def --destructure-map identity)'),
                                 (IDS[3], '(def reverse rseq)'), (IDS[4], '(def vector vec)'),
                                 (IDS[5], '(def some (fn [pred coll] (boolean (pred (first coll)))))')]:
            patches = copy.deepcopy(self.patches)
            patches[identity]['replacement'] = source
            with self.subTest(identity=identity), self.assertRaises(ValueError): verify(self.recipe, patches)
