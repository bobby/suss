"""Regression tests for strict #245 pinned-reference/raw-native validation."""
import copy
import json
from pathlib import Path
import unittest

import reduction_dispatch_oracle as oracle


class ReductionDispatchOracleTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        (cls.corpus, cls.original_cases, cls.order_cases,
         cls.original_capture, cls.order_capture, cls.reference) = oracle.load_inputs()

    def test_corpus_keeps_original_sources_and_adds_three_pinned_supplements(self):
        self.assertEqual(tuple(row['id'] for row in self.corpus['cases']), oracle.IDS)
        self.assertEqual(tuple(row['id'] for row in self.corpus['source_order_supplement']['cases']), oracle.ORDER_IDS)
        for case_id in oracle.ORDER_IDS:
            self.assertEqual(self.order_cases[case_id]['source_sha256'],
                             oracle.ORDER_SOURCE_SHA256[oracle.ORDER_IDS.index(case_id)])
            self.assertEqual(self.reference[case_id], oracle.order_expected()[case_id])

    def test_actual_seven_case_native_capture_matches_in_runtime_and_macro(self):
        native = oracle.read(oracle.NATIVE_ORIGINAL_PATH)
        matches = oracle.validate_observations(native, self.reference, oracle.IDS, 'packaged-original-capture')
        self.assertEqual(len(matches), 14)
        self.assertTrue(all(row['matched'] for row in matches))

    def test_supplement_is_reference_captured_and_native_validation_requires_all_ten(self):
        self.assertEqual(tuple(row['id'] for row in self.order_capture['cases']), oracle.ORDER_IDS)
        synthetic = oracle.reference_envelope(self.reference)
        matches = oracle.validate_observations(synthetic, self.reference, oracle.ALL_IDS, 'synthetic-validator-control')
        self.assertEqual(len(matches), 20)

    def test_adversarial_negative_controls_are_all_rejected(self):
        controls = oracle.negative_controls(self.reference)
        expected = {
            'missing-case-id', 'duplicate-map-entry-count-preserved', 'map-entry-count',
            'wrong-record-seed', 'callback-after-reduced', 'queue-fifo-order',
            'binary64-bit-change', 'source-pin', 'changed-historical-native-capture',
        }
        self.assertEqual({item['name'] for item in controls}, expected)
        self.assertTrue(all(item['rejected'] for item in controls))

    def test_historical_native_capture_sha_is_checked_before_decode(self):
        raw = oracle.NATIVE_ORIGINAL_PATH.read_bytes()
        self.assertEqual(oracle.digest(oracle.NATIVE_ORIGINAL_PATH), oracle.EXPECTED_ORIGINAL_NATIVE_SHA256)
        with self.assertRaisesRegex(AssertionError, 'historical original-seven native capture SHA-256 mismatch'):
            oracle.validate_pinned_native_capture_bytes(raw[:-1] + bytes([raw[-1] ^ 1]))

    def test_native_source_order_case_cannot_be_omitted_or_reordered(self):
        synthetic = oracle.reference_envelope(self.reference)
        synthetic['phases'][0]['cases'].pop()
        with self.assertRaises(AssertionError):
            oracle.validate_observations(synthetic, self.reference)
        synthetic = oracle.reference_envelope(self.reference)
        synthetic['phases'][1]['cases'][-1], synthetic['phases'][1]['cases'][-2] = (
            synthetic['phases'][1]['cases'][-2], synthetic['phases'][1]['cases'][-1])
        with self.assertRaises(AssertionError):
            oracle.validate_observations(synthetic, self.reference)


if __name__ == '__main__':
    unittest.main()
