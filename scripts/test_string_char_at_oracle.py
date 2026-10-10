import copy
import json
from pathlib import Path
import tempfile
import unittest
import string_char_at_oracle as gate

class CharAtRawEvidenceTests(unittest.TestCase):
    def test_closed_complete_source_and_independent_raw_utf16(self):
        gate.compare(gate.EVIDENCE)
        raw={x['id']:x['value'] for x in gate.observations(gate.ROOT/'docs/compatibility/string-char-at/evidence/primary-observations.json')}
        self.assertEqual(len(raw),74)
        for key in ['get-suite-13','get-suite-28']:
            self.assertEqual(raw[key]['items'][0]['items'], [{'tag':'bool','value':False},{'tag':'string','units':[97]}])
        self.assertEqual(raw['char-at-utf16-one']['items'][0]['items'][1], {'tag':'string','units':[55357]})
        self.assertEqual(raw['char-at-utf16-two']['items'][0]['items'][1], {'tag':'string','units':[56832]})
        self.assertEqual(raw['char-at-ascii-huge']['items'][0]['items'][1], {'tag':'string','units':[]})
        self.assertEqual(raw['char-at-ascii-nan']['items'][0]['items'][1], {'tag':'string','units':[97]})

    def test_source_order_scalar_throw_independent_counter_and_extra_argument(self):
        raw={x['id']:x['value'] for x in gate.observations(gate.ROOT/'docs/compatibility/string-char-at/evidence/primary-observations.json')}
        for key in ['char-at-throw-index','get-two-throw-index','get-three-throw-index']:
            self.assertEqual(raw[key]['items'][0]['items'], [{'tag':'bool','value':True},{'tag':'f64','bits':'4086080000000000'}]) #705
            self.assertEqual(raw[key]['items'][1],{'tag':'f64','bits':'3ff0000000000000'})
        self.assertEqual(raw['char-at-borrow-object-ordered']['items'][1],{'tag':'f64','bits':'4035000000000000'}) #21 receiver then index
        self.assertEqual(raw['char-at-borrow-receiver-throw']['items'][1],{'tag':'f64','bits':'4000000000000000'}) #2 no index conversion
        self.assertEqual(raw['char-at-extra-argument']['items'][1],{'tag':'f64','bits':'401c000000000000'}) #7

    def test_missing_reordered_duplicate_unknown_opaque_and_extra_fields_rejected(self):
        original=json.loads((gate.ROOT/'docs/compatibility/string-char-at/evidence/primary-observations.json').read_text())
        for mode in ['missing','reordered','duplicate','unknown','opaque','extra']:
            raw=copy.deepcopy(original)
            if mode=='missing':raw.pop()
            elif mode=='reordered':raw.reverse()
            elif mode=='duplicate':raw[1]=copy.deepcopy(raw[0])
            elif mode=='unknown':raw[0]['id']='unreviewed'
            elif mode=='opaque':raw[0]['value']={'tag':'opaque'}
            else:raw[0]['extra']='success'
            with tempfile.TemporaryDirectory() as directory:
                path=Path(directory)/'observations.json';path.write_text(json.dumps(raw))
                with self.assertRaises(ValueError):gate.observations(path)

if __name__=='__main__':unittest.main()
