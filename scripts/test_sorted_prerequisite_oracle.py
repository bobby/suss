import copy
import contextlib
import io
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
SPEC = importlib.util.spec_from_file_location('sorted_prerequisite_oracle', Path(__file__).with_name('sorted_prerequisite_oracle.py'))
oracle = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(oracle)

class ClosedSortedPrerequisiteEvidence(unittest.TestCase):
    def test_missing_reordered_duplicate_unknown_observations_rejected(self):
        good = [{'id': c['id'], 'value': True} for c in oracle.cases()]
        wrong = copy.deepcopy(good); wrong[0]['id'] = 'unknown'
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'observations.json'
            for rows in [good[:-1], list(reversed(good)), [good[0]] + good[:-1], wrong]:
                path.write_text(json.dumps(rows))
                with self.assertRaises(ValueError): oracle.compare(path)
            path.write_text(json.dumps(good))
            with contextlib.redirect_stdout(io.StringIO()): oracle.compare(path)

    def test_numeric_booleans_extra_fields_and_duplicate_keys_rejected(self):
        good = [{'id': c['id'], 'value': True} for c in oracle.cases()]
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'observations.json'
            for value in [1, 1.0, 0, None]:
                rows = copy.deepcopy(good); rows[0]['value'] = value
                path.write_text(json.dumps(rows))
                with self.assertRaises(ValueError): oracle.compare(path)
            rows = copy.deepcopy(good); rows[0]['extra'] = True
            path.write_text(json.dumps(rows))
            with self.assertRaises(ValueError): oracle.compare(path)
            path.write_text('{"x":1,"x":2}')
            with self.assertRaises(ValueError): oracle.read(path)

    def test_corpus_order_and_boolean_types_rejected(self):
        good = oracle.read(oracle.ROOT / 'tests/oracle/sorted-prerequisite-cases.json')
        mutations = []
        for value in [1, 1.0]:
            rows = copy.deepcopy(good); rows['cases'][0]['expected']['value'] = value; mutations.append(rows)
        rows = copy.deepcopy(good); rows['cases'].reverse(); mutations.append(rows)
        for rows in mutations:
            with patch.object(oracle, 'read', return_value=rows):
                with self.assertRaises(ValueError): oracle.cases()

    def test_malformed_source_extra_schema_and_false_observation_rejected(self):
        good = oracle.read(oracle.ROOT / 'tests/oracle/sorted-prerequisite-cases.json')
        mutations = []
        for source in ['(let [', '(identity 1) (identity 2)', '']:
            rows = copy.deepcopy(good); rows['cases'][0]['source'] = source; mutations.append(rows)
        rows = copy.deepcopy(good); rows['cases'][0]['extra'] = True; mutations.append(rows)
        rows = copy.deepcopy(good); rows['cases'][0]['expected']['value'] = False; mutations.append(rows)
        for rows in mutations:
            with patch.object(oracle, 'read', return_value=rows):
                with self.assertRaises(ValueError): oracle.cases()
        actual = [{'id': c['id'], 'value': True} for c in oracle.cases()]
        actual[0]['value'] = False
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'observations.json'; path.write_text(json.dumps(actual))
            with self.assertRaises(ValueError): oracle.compare(path)

    def test_stale_driver_rejected(self):
        with patch.object(oracle, 'driver', return_value='stale'):
            with self.assertRaises(ValueError): oracle.check()

if __name__ == '__main__': unittest.main()
