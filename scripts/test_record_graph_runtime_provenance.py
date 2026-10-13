import copy
import json
import unittest
from check_record_graph_runtime_provenance import ROOT, IDS, group_start, verify

class RuntimeGraphProvenance(unittest.TestCase):
    def setUp(self):
        self.recipe = json.loads((ROOT / 'docs/compatibility/core-import.json').read_text())
        start = group_start(self.recipe)
        self.patches = {entry['id']:json.loads((ROOT / entry['patch']).read_text()) for entry in self.recipe['forms'][start:start + len(IDS)]}

    def test_whole_closed_group(self):
        self.assertEqual(verify(self.recipe,self.patches),10)

    def test_missing_reordered_duplicate_and_wrong_hash_rejected(self):
        for change in ('missing','reordered','duplicate','hash'):
            recipe = copy.deepcopy(self.recipe)
            start = group_start(recipe)
            if change == 'missing': recipe['forms'].pop(start + len(IDS) - 1)
            elif change == 'reordered': recipe['forms'][start:start + len(IDS)] = recipe['forms'][start:start + len(IDS)][::-1]
            elif change == 'duplicate': recipe['forms'].append(copy.deepcopy(recipe['forms'][start]))
            else: recipe['forms'][start]['source-sha256'] = '0'*64
            with self.subTest(change=change),self.assertRaises(ValueError): verify(recipe,self.patches)

    def test_removed_branch_or_wrong_counter_declaration_rejected(self):
        for identity,replacement in [(IDS[0],'(def comp identity)'),(IDS[2],'(def juxt vector)'),(IDS[8],'(defonce gensym_counter nil)'),(IDS[9],'(def gensym (fn [] nil))')]:
            patches = copy.deepcopy(self.patches)
            patches[identity]['replacement'] = replacement
            with self.subTest(identity=identity),self.assertRaises(ValueError): verify(self.recipe,patches)
