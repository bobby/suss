import unittest

from performance_bench import PROGRAMS, evaluate, quiet_since


def measured():
    programs = PROGRAMS
    return {
        "receipt": {"completed": True, "source_dirty": "", "clojure_version": "1.12.6",
                    "n": 5000000, "reps": 3, "load_samples": [1.1, 2.2],
                    "start_load": [0.2, 0.3, 0.4], "bench_sha256": "same-source",
                    "idle_receipt": {"ready": True, "threshold": 0.75, "required_seconds": 60},
                    "programs": [[name, "unchanged form"] for name in programs]},
        "suss": {"times": {name: [4e-5, 1e-6] for name in programs},
                 "allocated": {"string-build": 61000}},
        "jvm": {"times": {name: [1e-5, 1e-7] for name in programs}},
    }


class PerformanceEvidenceTests(unittest.TestCase):
    def test_idle_duration_resets_on_busy_samples_and_includes_threshold(self):
        self.assertEqual(quiet_since(None, 100, 0.75), 100)
        self.assertEqual(quiet_since(100, 160, 0.5), 100)
        self.assertIsNone(quiet_since(100, 161, 0.7501))
        self.assertEqual(quiet_since(None, 162, 0.1), 162)

    def test_passing_data_keeps_exact_ratios_and_measured_spreads(self):
        result = evaluate(measured(), measured())
        self.assertTrue(result["timing_and_allocation_pass"])
        self.assertEqual(result["ratios"]["lazy-pipeline"], 4)
        self.assertEqual(result["string_bytes_per_run"], 61000)

    def test_noisy_incomplete_dirty_or_wrong_oracle_never_passes(self):
        for key, value in [("completed", False), ("source_dirty", " M compiler.rs"),
                           ("clojure_version", "1.12.5"), ("n", 1), ("reps", 1),
                           ("load_samples", []), ("load_samples", [3.00001]),
                           ("start_load", [0.751, 0, 0]), ("bench_sha256", "changed"),
                           ("programs", []), ("idle_receipt", {})]:
            for side in [0, 1]:
                pair = [measured(), measured()]
                pair[side]["receipt"][key] = value
                self.assertFalse(evaluate(*pair)["timing_and_allocation_pass"], (key, side))

    def test_allocation_limit_and_unknown_measurements_fail_closed(self):
        for size in [None, -1, 200000, float("nan"), float("inf")]:
            candidate = measured()
            candidate["suss"]["allocated"]["string-build"] = size
            self.assertFalse(evaluate(measured(), candidate)["timing_and_allocation_pass"])

    def test_exact_five_x_limit_does_not_use_a_rounded_table_ratio(self):
        baseline = measured()
        baseline["suss"]["times"]["lazy-pipeline"] = [6e-5, 1e-6]
        candidate = measured()
        candidate["suss"]["times"]["lazy-pipeline"][0] = 5e-5
        self.assertTrue(evaluate(baseline, candidate)["timing_and_allocation_pass"])
        candidate["suss"]["times"]["lazy-pipeline"][0] = 5.000001e-5
        self.assertFalse(evaluate(baseline, candidate)["timing_and_allocation_pass"])

    def test_zero_below_resolution_and_nonfinite_times_never_pass(self):
        for times in [[0, 0], [1e-5, 1e-5], [float("nan"), 0], [float("inf"), 0], [1e-5, -1]]:
            for runtime in ["suss", "jvm"]:
                candidate = measured()
                candidate[runtime]["times"]["string-build"] = times
                self.assertFalse(evaluate(measured(), candidate)["timing_and_allocation_pass"])

    def test_other_program_regressions_use_both_spreads(self):
        baseline, candidate = measured(), measured()
        candidate["suss"]["times"]["fib"] = [4.1e-5, 1e-6]
        self.assertTrue(evaluate(baseline, candidate)["timing_and_allocation_pass"])
        candidate["suss"]["times"]["fib"][0] = 4.3e-5
        self.assertFalse(evaluate(baseline, candidate)["timing_and_allocation_pass"])


if __name__ == "__main__":
    unittest.main()
