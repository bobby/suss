"""Integrity gates prevent shipping a stale opaque numeric conversion artifact."""
import json
from pathlib import Path
import shutil
import tempfile
import unittest
import numeric_runtime as numeric


class NumericArtifactTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / 'numeric'
        shutil.copytree(numeric.ROOT, self.root, ignore=shutil.ignore_patterns('target'))

    def test_tracked_sources_dependency_licenses_and_artifact_match(self):
        numeric.verify(self.root)

    def test_changed_source_cannot_keep_old_artifact_manifest(self):
        source = self.root / 'src/lib.rs'
        source.write_text(source.read_text() + '\n// changed source\n')
        with self.assertRaisesRegex(ValueError, 'stale'):
            numeric.verify(self.root)

    def test_changed_wasm_cannot_keep_old_manifest(self):
        binary = self.root / 'artifact/numeric.wasm'
        data = bytearray(binary.read_bytes())
        data[-1] ^= 1
        binary.write_bytes(data)
        with self.assertRaisesRegex(ValueError, 'stale'):
            numeric.verify(self.root)

    def test_duplicate_extra_and_boolean_schema_fields_fail(self):
        path = self.root / 'artifact/manifest.json'
        original = path.read_text()
        with self.assertRaisesRegex(ValueError, 'duplicate'):
            path.write_text('{"schema": 1, "schema": 1}')
            numeric.verify(self.root)
        for modification in ({'schema': True}, {'extra': 1}):
            value = json.loads(original)
            value.update(modification)
            path.write_text(json.dumps(value))
            with self.assertRaisesRegex(ValueError, 'malformed'):
                numeric.verify(self.root)
