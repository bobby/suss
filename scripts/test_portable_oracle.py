import contextlib
import io
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
import portable_oracle as oracle


class PortableOracleTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix='suss-portable-oracle-test-')
        self.addCleanup(self.directory.cleanup)
        self.corpus_path = Path(self.directory.name) / 'corpus.json'
        self.observations_path = Path(self.directory.name) / 'observations.json'
        self.corpus = {'schema': 1, 'upstream': oracle.PIN, 'cases': [
            {'id': 'boolean', 'source': 'false', 'expected': {'tag': 'bool', 'value': False}}]}
        self.observations = {'schema': 1, 'upstream': oracle.PIN, 'cases': [
            {'id': 'boolean', 'value': {'tag': 'bool', 'value': False}}]}
        self.corpus_path.write_text(json.dumps(self.corpus))
        self.observations_path.write_text(json.dumps(self.observations))
        self.addCleanup(patch.stopall)
        patch.object(oracle, 'CORPUS', self.corpus_path).start()
        patch.object(oracle, 'OBSERVATIONS', self.observations_path).start()

    def compare(self):
        with contextlib.redirect_stdout(io.StringIO()):
            oracle.compare()

    def test_complete_result_passes_but_absent_result_is_not_success(self):
        self.compare()
        self.observations_path.unlink()
        with self.assertRaises(FileNotFoundError):
            self.compare()

    def test_wrong_pin_schema_and_missing_or_duplicate_source_fail(self):
        for replacement in [
            {'upstream': 'wrong'}, {'schema': True}, {'cases': []},
            {'cases': self.corpus['cases'] * 2},
            {'cases': [{'id': 'empty', 'source': '', 'expected': {'tag': 'nil'}}]},
        ]:
            with self.subTest(replacement=replacement):
                self.corpus_path.write_text(json.dumps({**self.corpus, **replacement}))
                with self.assertRaises(ValueError):
                    oracle.load()

    def test_missing_duplicate_and_changed_observations_fail(self):
        for cases in [[], self.observations['cases'] * 2,
                      [{'id': 'different', 'value': {'tag': 'bool', 'value': False}}],
                      [{'id': 'boolean', 'value': {'tag': 'bool', 'value': True}}]]:
            self.observations_path.write_text(json.dumps({**self.observations, 'cases': cases}))
            with self.assertRaises(ValueError):
                self.compare()

    def test_boolean_schema_and_values_never_compare_as_integer_success(self):
        self.observations_path.write_text(json.dumps({**self.observations, 'schema': True}))
        with self.assertRaises(ValueError):
            self.compare()
        observations = {**self.observations, 'cases': [
            {'id': 'boolean', 'value': {'tag': 'bool', 'value': 0}}]}
        self.observations_path.write_text(json.dumps(observations))
        with self.assertRaises(ValueError):
            self.compare()

    def test_extra_fields_duplicate_keys_and_trailing_data_fail(self):
        for content in [json.dumps({**self.observations, 'extra': 'unknown'}),
                        '{"schema":1,"schema":1,"upstream":"' + oracle.PIN + '","cases":[]}',
                        json.dumps(self.observations) + ' nil']:
            self.observations_path.write_text(content)
            with self.assertRaises(ValueError):
                self.compare()
