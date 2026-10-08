import unittest
from core_import import adapt_form, extract_forms, json_data, verify_outputs


class CoreImportTests(unittest.TestCase):
    def test_exact_utf8_ranges_keep_metadata_and_reader_context(self):
        source = '; 😀\n#?(:cljs (defn ^:private x [v] v)\n :clj (defmacro x [v] v))'
        forms = extract_forms(source, 'runtime', 'core.cljc')
        self.assertEqual(list(forms), ['runtime:x:2', 'runtime:x:3'])
        record = forms['runtime:x:2']
        self.assertEqual(record['context'], [':cljs'])
        self.assertEqual(source.encode()[record['byte-start']:record['byte-end']],
                         record['form'].encode())
        self.assertEqual(record['form'], '(defn ^:private x [v] v)')

    def test_patch_is_bound_to_exact_source_and_preserves_identity(self):
        original = '(defn identity [x] x)'
        import hashlib
        digest = hashlib.sha256(original.encode()).hexdigest()
        patch = {'schema': 1, 'source-sha256': digest,
                 'replacement': '(def identity (fn [x] x))',
                 'rationale': 'Explicit bootstrap defn adaptation.'}
        self.assertEqual(adapt_form(original, 'identity', patch), patch['replacement'])
        for change in [dict(patch, **{'source-sha256': '0' * 64}),
                       dict(patch, replacement='(def other 1)'),
                       dict(patch, replacement='(def identity 1) (def hidden 2)'),
                       dict(patch, replacement="'(def identity 1)"),
                       dict(patch, extra=True), dict(patch, rationale='')]:
            with self.assertRaises(ValueError):
                adapt_form(original, 'identity', change)

    def test_discarded_and_quoted_forms_are_never_extracted(self):
        forms = extract_forms("#_(def dropped 1) '(def quoted 2) (def kept 3)",
                              'runtime', 'core.cljs')
        self.assertEqual(list(forms), ['runtime:kept:1'])

    def test_duplicate_json_fields_are_rejected(self):
        with self.assertRaisesRegex(ValueError, 'duplicate JSON key'):
            json_data('{"source-sha256":"old","source-sha256":"new"}')

    def test_generated_file_tampering_missing_files_and_extras_fail(self):
        import tempfile
        from pathlib import Path
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            outputs = {'core.sus': b'(def identity (fn [x] x))', 'LICENSE': b'EPL'}
            for path, value in outputs.items():
                (root / path).write_bytes(value)
            verify_outputs(root, outputs)
            (root / 'core.sus').write_bytes(b'(def identity (fn [x] 42))')
            with self.assertRaisesRegex(ValueError, 'stale core import'):
                verify_outputs(root, outputs)
            (root / 'core.sus').write_bytes(outputs['core.sus'])
            (root / 'LICENSE').unlink()
            with self.assertRaisesRegex(ValueError, 'stale core import'):
                verify_outputs(root, outputs)
            (root / 'LICENSE').write_bytes(outputs['LICENSE'])
            (root / 'hidden.sus').write_bytes(b'(def hidden 1)')
            with self.assertRaisesRegex(ValueError, 'unexpected generated files'):
                verify_outputs(root, outputs)

class CoreImportBuildTests(unittest.TestCase):
    def setUp(self):
        import tempfile
        from pathlib import Path
        from unittest.mock import patch
        import core_import as importer
        self.importer = importer
        self.original_pinned_file = importer.pinned_file
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        notice = '; Copyright (c) Rich Hickey.\n; Eclipse Public License 1.0\n'
        source = notice + '\n(defn identity [x] x)\n'
        for relative in importer.SOURCES:
            p = self.root / 'clojurescript' / relative
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(source)
        for name in ('LICENSE', 'epl-v10.html'):
            (self.root / 'clojurescript' / name).write_text('fixture EPL')
        import hashlib, json
        self.identity = 'runtime:identity:4'
        self.digest = hashlib.sha256(b'(defn identity [x] x)').hexdigest()
        self.recipe = {'schema': 1, 'upstream-commit': importer.PIN,
                       'namespace': 'suss.core', 'phase': 'runtime',
                       'forms': [{'id': self.identity, 'source-sha256': self.digest, 'patch': None}]}
        self.recipe_path = self.root / importer.RECIPE
        self.recipe_path.parent.mkdir(parents=True, exist_ok=True)
        self.recipe_path.write_text(json.dumps(self.recipe))
        self.reviews = self.root / 'docs/compatibility/reviews.edn'
        self.reviews.write_text('''{:schema 1 :upstream-commit "%s" :reviews {
          "%s" {:source-sha256 "%s" :visibility :public
          :arities {:fixed [1] :variadic-min nil} :dependencies []
          :classification :portable :rationale "fixture" :status :in-progress
          :adaptation-path nil :alternative nil :tests []}}}''' % (importer.PIN, self.identity, self.digest))
        (self.root / 'docs/compatibility/cljs-core.edn').write_text('fixture inventory')
        for name in ('cljs_inventory.py', 'cljs_reviews.py'):
            p = self.root / 'scripts' / name
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text('fixture tool')
        self.addCleanup(patch.stopall)
        patch.object(importer, 'ROOT', self.root).start()
        patch.object(importer, 'inventory_records', return_value=[{
            'id': self.identity, 'kind': 'defn', 'sha256': self.digest}]).start()
        patch.object(importer, 'generate', return_value=('fixture inventory', 1)).start()
        patch.object(importer, 'pinned_file', side_effect=lambda root, relative:
                     (root / 'clojurescript' / relative).read_bytes()).start()

    def build(self):
        return self.importer.build(self.root)

    def test_reproduction_retains_exact_form_notices_and_licenses(self):
        import json
        first, second = self.build(), self.build()
        self.assertEqual(first, second)
        self.assertIn(b'Copyright', first['suss/core.sus'])
        self.assertIn(b'(defn identity [x] x)', first['extracted/0000.cljs'])
        self.assertEqual(first['LICENSE'], b'fixture EPL')
        manifest = json.loads(first['manifest.json'])
        self.assertEqual(manifest['forms'][0]['source-sha256'], self.digest)
        self.assertIsNone(manifest['forms'][0]['patch'])

    def test_original_loader_is_ordered_hashed_and_rejects_unknown_stages(self):
        import hashlib, json
        inputs = {}
        # Deliberately reverse JSON order: execution still follows stage order.
        for stage in ('original', 'after', 'before'):
            path = self.root / f'{stage}.sus'
            text = f'(def {stage}-marker 1)\n'
            path.write_text(text)
            inputs[stage] = {'path': path.name,
                             'sha256': hashlib.sha256(text.encode()).hexdigest()}
        self.recipe['loader'] = inputs
        self.recipe_path.write_text(json.dumps(self.recipe))
        output = self.build()
        source = output['suss/core.sus'].decode()
        self.assertLess(source.index('before-marker'), source.index('(defn identity'))
        self.assertLess(source.index('(defn identity'), source.index('after-marker'))
        self.assertLess(source.index('after-marker'), source.index('original-marker'))
        manifest = json.loads(output['manifest.json'])
        self.assertEqual(manifest['loader']['original'], inputs['original'])
        (self.root / 'original.sus').write_text('(def tampered 1)')
        with self.assertRaisesRegex(ValueError, 'stale loader input hash: original'):
            self.build()
        inputs['unexpected'] = inputs['original']
        self.recipe_path.write_text(json.dumps(self.recipe))
        with self.assertRaises(ValueError):
            self.build()

    def test_loader_trailing_comments_do_not_swallow_following_stages(self):
        import hashlib, json
        from cljs_inventory import Scanner, declarations
        inputs = {}
        for stage in ('before', 'after', 'original'):
            path = self.root / f'{stage}.sus'
            raw = f'(def {stage}-marker 1) ; trailing comment'.encode()
            path.write_bytes(raw)
            inputs[stage] = {'path': path.name,
                             'sha256': hashlib.sha256(raw).hexdigest()}
        self.recipe['loader'] = inputs
        self.recipe_path.write_text(json.dumps(self.recipe))
        source = self.build()['suss/core.sus'].decode()
        names = [name for form in Scanner(source).all()
                 for _, name, _, _ in declarations(form)]
        self.assertEqual(names, ['before-marker', 'identity', 'after-marker',
                                 'original-marker'])

    def test_stale_review_and_unreviewed_selection_fail(self):
        original = self.reviews.read_text()
        self.reviews.write_text(original.replace(self.digest, '0' * 64))
        with self.assertRaisesRegex(ValueError, 'stale source hash'):
            self.build()
        self.reviews.write_text(original.replace('"' + self.identity + '"', '"unknown"'))
        with self.assertRaisesRegex(ValueError, 'unknown review declaration'):
            self.build()

    def test_selection_without_review_is_rejected(self):
        self.reviews.write_text('{:schema 1 :upstream-commit "' + self.importer.PIN + '" :reviews {}}')
        with self.assertRaisesRegex(ValueError, 'unreviewed declaration'):
            self.build()


    def test_recipe_duplicate_phase_stale_hash_and_missing_notice_fail(self):
        import json
        for replacement in [dict(self.recipe, forms=self.recipe['forms'] * 2),
                            dict(self.recipe, phase='macro'),
                            dict(self.recipe, forms=[dict(self.recipe['forms'][0], **{'source-sha256': '0' * 64})])]:
            self.recipe_path.write_text(json.dumps(replacement))
            with self.assertRaises(ValueError):
                self.build()
        self.recipe_path.write_text(json.dumps(self.recipe))
        (self.root / 'clojurescript' / self.importer.SOURCES[0]).write_text('(defn identity [x] x)')
        with self.assertRaisesRegex(ValueError, 'missing upstream notice'):
            self.build()

    def test_contextual_declarations_are_not_flattened_into_unconditional_source(self):
        source_path = self.root / 'clojurescript' / self.importer.SOURCES[0]
        original = source_path.read_text()
        # Both preserve the selected form/hash/line while changing its execution
        # context: a nonportable reader branch, or a lexical capture.
        for contextual in ['#?(:clj (defn identity [x] x))',
                           '(let [captured 42] (defn identity [x] captured))']:
            source_path.write_text(original[:original.index('(defn')] + contextual + '\n')
            if 'captured' in contextual:
                import hashlib, json
                original_hash = self.digest
                self.digest = hashlib.sha256(b'(defn identity [x] captured)').hexdigest()
                self.recipe['forms'][0]['source-sha256'] = self.digest
                self.recipe_path.write_text(json.dumps(self.recipe))
                self.reviews.write_text(self.reviews.read_text().replace(original_hash, self.digest))
                from unittest.mock import patch
                patch.object(self.importer, 'inventory_records', return_value=[{
                    'id': self.identity, 'kind': 'defn', 'sha256': self.digest}]).start()
            with self.subTest(context=contextual), self.assertRaisesRegex(ValueError, 'unsupported declaration context'):
                self.build()

    def test_loader_inputs_are_ordered_hashed_and_cannot_escape_repository(self):
        import hashlib, json
        before = self.root / 'loader-before.sus'
        after = self.root / 'loader-after.sus'
        before.write_text('(declare identity)\n')
        after.write_text('(identity 7)\n')
        loader = {stage: {'path': path.name, 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}
                  for stage, path in [('before', before), ('after', after)]}
        self.recipe_path.write_text(json.dumps(dict(self.recipe, loader=loader)))
        built = self.build()
        source = built['suss/core.sus'].decode()
        self.assertLess(source.index('(declare identity)'), source.index('(defn identity'))
        self.assertLess(source.index('(defn identity'), source.index('(identity 7)'))
        self.assertEqual(json.loads(built['manifest.json'])['loader'], loader)
        after.write_text('(identity 8)\n')
        with self.assertRaisesRegex(ValueError, 'stale loader input hash'):
            self.build()
        loader['after']['path'] = '../outside'
        self.recipe_path.write_text(json.dumps(dict(self.recipe, loader=loader)))
        with self.assertRaisesRegex(ValueError, 'missing or escaping'):
            self.build()

    def test_license_path_escaping_repository_is_rejected(self):
        with self.assertRaises(ValueError):
            self.importer.repository_file(self.root, '../outside')

    def test_modified_upstream_license_does_not_become_new_provenance(self):
        from unittest.mock import patch
        import subprocess
        with patch.object(subprocess, 'check_output', return_value=b'original EPL'):
            with self.assertRaisesRegex(ValueError, 'differs from pin'):
                self.original_pinned_file(self.root, 'LICENSE')


if __name__ == '__main__':
    unittest.main()
