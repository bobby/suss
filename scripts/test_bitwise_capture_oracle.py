"""Diagnostic results stay exact and separate from equal public observations."""
from copy import deepcopy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import bitwise_capture_oracle as capture


class BitwiseCaptureOracleTests(unittest.TestCase):
    def test_malformed_missing_duplicate_or_changed_diagnostic_results_fail(self):
        corpus = capture.load()
        expected = {'schema': 1, 'upstream': corpus['upstream'],
                    'cases': [{'id': c['id'], 'value': c['expected-primary']} for c in corpus['cases']]}
        bad = []
        for change in [lambda d: d.update(schema=True),
                       lambda d: d['cases'].pop(),
                       lambda d: d['cases'].append(d['cases'][0]),
                       lambda d: d['cases'][0]['value']['units'].append(33),
                       lambda d: d['cases'][-1]['value'].update(bits='0000000000000000')]:
            document = deepcopy(expected)
            change(document)
            bad.append(document)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'observations.json'
            with patch.object(capture, 'OBSERVATIONS', path):
                for document in bad:
                    path.write_text(json.dumps(document))
                    with self.subTest(document=document), self.assertRaises(ValueError):
                        capture.compare()
