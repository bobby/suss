import copy
import tempfile
import tarfile
import unittest
from pathlib import Path

import json

from wasi_lock import LOCK, SOURCES, manifest, sha256, verify, verify_archive


class WasiLockTests(unittest.TestCase):
    def test_official_release_matches_lock(self):
        verify(SOURCES, json.loads(LOCK.read_text()))

    def test_archive_hash_and_contents(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.fixture(root)
            lock = manifest(root)
            archive_path = root / 'release.tar.gz'
            with tarfile.open(archive_path, 'w:gz') as archive:
                for name in lock['files']:
                    archive.add(root / name, arcname='wasi-wit-0.3.1/' + name)
            lock['archive_sha256'] = sha256(archive_path.read_bytes())
            verify_archive(archive_path, lock)
            changed = copy.deepcopy(lock)
            changed['files']['dep/dep.wit']['sha256'] = '0' * 64
            with self.assertRaises(ValueError):
                verify_archive(archive_path, changed)
            archive_path.write_bytes(archive_path.read_bytes() + b'changed')
            with self.assertRaises(ValueError):
                verify_archive(archive_path, lock)

    def fixture(self, root):
        (root / 'dep').mkdir()
        (root / 'app' / 'deps').mkdir(parents=True)
        dep = b'package wasi:dep@0.2.7;\ninterface api {}\n'
        (root / 'dep' / 'dep.wit').write_bytes(dep)
        (root / 'app' / 'deps' / 'dep.wit').write_bytes(dep)
        (root / 'app' / 'app.wit').write_text(
            'package wasi:app@0.3.1;\ninterface local { use wasi:dep/api@0.2.7.{item}; }\nworld command { import wasi:dep/api@0.2.7; }\n')

    def test_preserves_dependency_version(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.fixture(root)
            lock = manifest(root)
            self.assertEqual(lock['packages']['wasi:app@0.3.1']['dependencies'], ['wasi:dep@0.2.7'])
            verify(root, lock)

    def test_missing_changed_and_extra_files_fail(self):
        for mutation in ('missing', 'changed', 'extra'):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                self.fixture(root)
                lock = manifest(root)
                path = root / 'dep' / 'dep.wit'
                if mutation == 'missing':
                    path.unlink()
                elif mutation == 'changed':
                    path.write_bytes(path.read_bytes() + b'// changed\n')
                else:
                    (root / 'dep' / 'extra.wit').write_text('package wasi:extra@0.3.1;')
                with self.assertRaises(ValueError):
                    verify(root, lock)

    def test_dependency_subset_is_preserved_and_locked(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.fixture(root)
            (root / 'app' / 'deps' / 'dep.wit').write_text('package wasi:dep@0.2.7;\ninterface changed {}\n')
            lock = manifest(root)
            self.assertNotEqual(lock['files']['app/deps/dep.wit']['sha256'],
                                lock['files']['dep/dep.wit']['sha256'])
            verify(root, lock)

    def test_missing_dependency_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.fixture(root)
            (root / 'app' / 'deps' / 'dep.wit').unlink()
            with self.assertRaises(ValueError):
                manifest(root)

    def test_changed_lock_graph_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.fixture(root)
            lock = copy.deepcopy(manifest(root))
            lock['packages']['wasi:app@0.3.1']['dependencies'] = []
            with self.assertRaises(ValueError):
                verify(root, lock)
