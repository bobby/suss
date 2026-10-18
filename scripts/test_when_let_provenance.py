import difflib
import hashlib
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import check_when_let_provenance as provenance


class CapturedFixtureTests(unittest.TestCase):
    def setUp(self):
        self.workspace = tempfile.TemporaryDirectory()
        self.addCleanup(self.workspace.cleanup)
        self.root = Path(self.workspace.name)
        self.directory = self.root / 'provenance'
        self.directory.mkdir()
        self.fixture = 'fixture.rs'
        original = b'old diagnostic\nunchanged guest\n'
        current = b'stage diagnostic\nunchanged guest\n'
        self.original = self.directory / 'original.rs'
        self.original.write_bytes(original)
        (self.root / self.fixture).write_bytes(current)
        delta = ''.join(difflib.unified_diff(
            original.decode().splitlines(True), current.decode().splitlines(True),
            fromfile='captured/' + self.fixture, tofile='current/' + self.fixture)).encode()
        self.delta = self.directory / 'fixture.patch'
        self.delta.write_bytes(delta)
        digest = lambda value: hashlib.sha256(value).hexdigest()
        self.record = {
            'files': {self.fixture: digest(current)},
            'native_fixture_adaptations': {self.fixture: {
                'original': 'original.rs', 'original_sha256': digest(original),
                'patch': 'fixture.patch', 'patch_sha256': digest(delta)}}}
        self.scope = patch.multiple(provenance, ROOT=self.root, DIRECTORY=self.directory)
        self.scope.start()
        self.addCleanup(self.scope.stop)

    def test_exact_adaptation_preserves_captured_hash(self):
        expected = self.record['native_fixture_adaptations'][self.fixture]['original_sha256']
        self.assertEqual(provenance.captured_fixture_hash(self.record, self.fixture), expected)

    def test_changed_current_fixture_rejects(self):
        (self.root / self.fixture).write_text('different guest\n')
        with self.assertRaises(AssertionError):
            provenance.captured_fixture_hash(self.record, self.fixture)

    def test_changed_captured_fixture_rejects(self):
        self.original.write_text('rewritten capture\n')
        with self.assertRaises(AssertionError):
            provenance.captured_fixture_hash(self.record, self.fixture)

    def test_missing_delta_rejects_even_with_updated_delta_hash(self):
        self.delta.write_bytes(b'')
        self.record['native_fixture_adaptations'][self.fixture]['patch_sha256'] = hashlib.sha256(b'').hexdigest()
        with self.assertRaises(AssertionError):
            provenance.captured_fixture_hash(self.record, self.fixture)

    def test_parent_path_rejects(self):
        self.record['native_fixture_adaptations'][self.fixture]['original'] = '../fixture.rs'
        with self.assertRaises(AssertionError):
            provenance.captured_fixture_hash(self.record, self.fixture)
