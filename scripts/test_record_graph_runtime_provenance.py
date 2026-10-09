import copy
import json
import unittest
from check_record_graph_runtime_provenance import ROOT, IDS, verify

class RuntimeGraphProvenance(unittest.TestCase):
    def setUp(self):
        self.recipe = json.loads((ROOT / 'docs/compatibility/core-import.json').read_text())
        self.patches = {entry['id']:json.loads((ROOT / entry['patch']).read_text()) for entry in self.recipe['forms'][340:350]}

    def test_whole_closed_group(self):
        self.assertEqual(verify(self.recipe,self.patches),10)

    def test_missing_reordered_duplicate_and_wrong_hash_rejected(self):
        for change in ('missing','reordered','duplicate','hash'):
            recipe = copy.deepcopy(self.recipe)
            if change == 'missing': recipe['forms'].pop()
            elif change == 'reordered': recipe['forms'][340:350] = recipe['forms'][340:350][::-1]
            elif change == 'duplicate': recipe['forms'].append(copy.deepcopy(recipe['forms'][340]))
            else: recipe['forms'][340]['source-sha256'] = '0'*64
            with self.subTest(change=change),self.assertRaises(ValueError): verify(recipe,self.patches)

    def test_removed_branch_or_wrong_counter_declaration_rejected(self):
        for identity,replacement in [(IDS[0],'(def comp identity)'),(IDS[2],'(def juxt vector)'),(IDS[8],'(defonce gensym_counter nil)'),(IDS[9],'(def gensym (fn [] nil))')]:
            patches = copy.deepcopy(self.patches)
            patches[identity]['replacement'] = replacement
            with self.subTest(identity=identity),self.assertRaises(ValueError): verify(self.recipe,patches)
