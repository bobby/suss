import json
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

from bootstrap_diagnostics import NAMES, retain

SCRIPTS = Path(__file__).resolve().parent


class BootstrapDiagnosticsTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.area = Path(self.temporary.name)
        self.root = self.area / "repo"
        shipped = self.root / "runtime" / "bootstrap"
        shipped.mkdir(parents=True)
        scripts = self.root / "scripts"
        scripts.mkdir()
        for name in ("verify-bootstrap.sh", "bootstrap_diagnostics.py"):
            shutil.copyfile(SCRIPTS / name, scripts / name)
        self.first, self.second = self.area / "first", self.area / "second"
        self.first.mkdir()
        self.second.mkdir()
        for name in NAMES:
            for directory in (shipped, self.first, self.second):
                (directory / name).write_bytes(f"{name}:fixture".encode())
        self.git("init", "-q")
        self.git("add", ".")
        self.git("-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "commit", "-qm", "fixture")

    def git(self, *args):
        return subprocess.check_output(["git", "-C", str(self.root), *args], stderr=subprocess.PIPE).strip()

    def test_receipt_keeps_both_reproductions_and_exact_source(self):
        destination = self.area / "retained"
        receipt = retain(self.root, self.first, self.second, destination)
        self.assertTrue(receipt["reproductions_match"])
        self.assertTrue(receipt["shipped_matches_first"])
        self.assertTrue(receipt["source"]["working_tree_clean"])
        self.assertEqual(receipt["source"]["head"], self.git("rev-parse", "HEAD").decode())
        self.assertEqual(len(receipt["files"]), 8)
        self.assertEqual(json.loads((destination / "receipt.json").read_text()), receipt)
        for label in ("first", "second"):
            for name in NAMES:
                self.assertEqual((destination / label / name).read_bytes(), (self.first / name).read_bytes())

    def test_nondeterminism_and_dirty_source_are_reported_not_promoted(self):
        (self.second / "macro.wasm").write_bytes(b"different fixture")
        (self.root / "runtime/bootstrap/runtime.wasm").write_bytes(b"changed shipped fixture")
        receipt = retain(self.root, self.first, self.second, self.area / "retained")
        self.assertFalse(receipt["reproductions_match"])
        self.assertFalse(receipt["shipped_matches_first"])
        self.assertFalse(receipt["source"]["working_tree_clean"])

    def test_incomplete_generation_or_existing_destination_cannot_publish_receipt(self):
        destination = self.area / "retained"
        (self.second / "macro.json").unlink()
        with self.assertRaises(FileNotFoundError):
            retain(self.root, self.first, self.second, destination)
        self.assertFalse(destination.exists())
        (self.second / "macro.json").write_bytes(b"fixture")
        destination.mkdir()
        sentinel = destination / "keep"
        sentinel.write_text("prior evidence")
        with self.assertRaises(FileExistsError):
            retain(self.root, self.first, self.second, destination)
        self.assertEqual(sentinel.read_text(), "prior evidence")
        self.assertFalse((destination / "receipt.json").exists())

    def test_shell_retains_outputs_and_still_fails_both_mismatch_kinds(self):
        # Synthetic generator only: no real Cargo, compiler or Wasm execution.
        tools = self.area / "stub-tools"
        tools.mkdir()
        cargo = tools / "cargo"
        cargo.write_text('''#!/bin/sh
set -eu
printf '%s\\n' "$*" >> "$SUSS_TEST_CARGO_CALLS"
if [ "$1" != run ]; then exit 99; fi
for last do :; done
mkdir -p "$last"
for phase in runtime macro; do
    for suffix in wasm json; do
        printf 'generated:%s:%s' "$phase" "$suffix" > "$last/$phase.$suffix"
    done
done
if [ "${SUSS_TEST_DIFFERENT_SECOND:-0}" = 1 ]; then
    case "$last" in */second) printf different >> "$last/runtime.wasm" ;; esac
fi
''')
        cargo.chmod(0o755)
        for different in (False, True):
            calls = self.area / f"calls-{different}"
            destination = self.area / f"diagnostic-{different}"
            environment = dict(os.environ, PATH=str(tools) + os.pathsep + os.environ["PATH"],
                TMPDIR=str(self.area), SUSS_BOOTSTRAP_DIAGNOSTICS_DIR=str(destination),
                SUSS_TEST_CARGO_CALLS=str(calls), SUSS_TEST_DIFFERENT_SECOND=str(int(different)))
            result = subprocess.run(["sh", "scripts/verify-bootstrap.sh"], cwd=self.root,
                env=environment, capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
            receipt = json.loads((destination / "receipt.json").read_text())
            self.assertEqual(receipt["reproductions_match"], not different)
            self.assertFalse(receipt["shipped_matches_first"])
            self.assertEqual(len(calls.read_text().splitlines()), 2)
            self.assertTrue(all(line.startswith("run ") for line in calls.read_text().splitlines()))


if __name__ == "__main__":
    unittest.main()
