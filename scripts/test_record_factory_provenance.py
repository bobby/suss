import copy
import re
import json
import unittest

from check_record_macro_provenance import ROOT, verify_factory_adaptations


class RecordFactoryProvenance(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.provenance = json.loads((ROOT / 'docs/compatibility/record-macro-foundations/factory-adaptations.json').read_text())
        cls.host = re.sub(r',(?=\))', '', re.sub(r'\s+', '', (ROOT / 'crates/suss-compile/src/portable_macros.rs').read_text()))

    def test_complete_source_adaptations(self):
        self.assertEqual(verify_factory_adaptations(self.provenance), 2)

    def test_stale_file_source_original_and_declaration_hashes_rejected(self):
        for location, key in [('top', 'source-file-sha256'), ('top', 'compiled-source-sha256'),
                              ('form', 'original-sha256'), ('form', 'adapted-declaration-sha256')]:
            with self.subTest(location=location, key=key):
                data = copy.deepcopy(self.provenance)
                target = data if location == 'top' else data['forms'][0]
                target[key] = '0' * 64
                with self.assertRaises(ValueError):
                    verify_factory_adaptations(data)

    def test_missing_reordered_duplicate_forms_rejected(self):
        for mutation in ['missing', 'reordered', 'duplicate']:
            with self.subTest(mutation=mutation):
                data = copy.deepcopy(self.provenance)
                if mutation == 'missing':
                    data['forms'].pop()
                elif mutation == 'reordered':
                    data['forms'].reverse()
                else:
                    data['forms'][1] = copy.deepcopy(data['forms'][0])
                with self.assertRaises(ValueError):
                    verify_factory_adaptations(data)

    def test_stale_missing_reordered_duplicate_helper_declarations_rejected(self):
        for mutation in ['stale', 'missing', 'reordered', 'duplicate']:
            with self.subTest(mutation=mutation):
                data = copy.deepcopy(self.provenance)
                helpers = data['helper-declarations']
                if mutation == 'stale':
                    helpers[0]['sha256'] = '0' * 64
                elif mutation == 'missing':
                    helpers.pop()
                elif mutation == 'reordered':
                    helpers.reverse()
                else:
                    helpers[1] = copy.deepcopy(helpers[0])
                with self.assertRaises(ValueError):
                    verify_factory_adaptations(data)

    def test_bootstrap_installation_and_artifact_dependency_required(self):
        for old in ['macros.session.eval(crate::portable_record_factories::SOURCE)?;',
                    'dependencies.push(("record-factory-source".into(), crate::portable::bootstrap::sha256(crate::portable_record_factories::SOURCE.as_bytes())));']:
            with self.subTest(old=old):
                old = re.sub(r'\s+', '', old)
                self.assertIn(old, self.host)
                with self.assertRaises(ValueError):
                    verify_factory_adaptations(self.provenance, self.host.replace(old, ''))


if __name__ == '__main__':
    unittest.main()
