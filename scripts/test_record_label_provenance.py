import copy
import re
import json
import unittest
from check_record_macro_provenance import ROOT, verify_label_adaptations


class RecordLabelProvenance(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.provenance = json.loads((ROOT / 'docs/compatibility/record-macro-foundations/label-adaptations.json').read_text())
        cls.host = re.sub(r',(?=\))', '', re.sub(r'\s+', '', (ROOT / 'crates/suss-compile/src/portable_macros.rs').read_text()))

    def test_complete_retained_inputs_and_source_identities(self):
        self.assertEqual(verify_label_adaptations(self.provenance), 4)

    def test_stale_hashes_missing_and_reordered_inputs_rejected(self):
        for mutation in ['file', 'source', 'original', 'missing', 'reordered']:
            with self.subTest(mutation=mutation):
                data = copy.deepcopy(self.provenance)
                if mutation == 'file':
                    data['source-file-sha256'] = '0' * 64
                elif mutation == 'source':
                    data['compiled-source-sha256'] = '0' * 64
                elif mutation == 'original':
                    data['originals'][0]['sha256'] = '0' * 64
                elif mutation == 'missing':
                    data['originals'].pop()
                else:
                    data['originals'].reverse()
                with self.assertRaises(ValueError):
                    verify_label_adaptations(data)

    def test_bootstrap_and_cache_identity_required(self):
        for old in ['macros.session.eval(crate::portable_record_label::SOURCE)?;',
                    'dependencies.push(("record-label-source".into(), crate::portable::bootstrap::sha256(crate::portable_record_label::SOURCE.as_bytes())));']:
            with self.subTest(old=old):
                old = re.sub(r'\s+', '', old)
                self.assertIn(old, self.host)
                with self.assertRaises(ValueError):
                    verify_label_adaptations(self.provenance, self.host.replace(old, ''))


if __name__ == '__main__':
    unittest.main()
