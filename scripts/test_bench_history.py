"""Keep published benchmark counts and units intact across table formats."""
import importlib.util
from pathlib import Path
import sys
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[1]
SCRIPTS = ROOT / 'experiments/numeric-representation/suss-jvm'
RESULTS = ROOT / 'experiments/numeric-representation/results'


def load_history():
    spec = importlib.util.spec_from_file_location('suss_bench_cases', SCRIPTS / 'bench.py')
    bench = importlib.util.module_from_spec(spec)
    # bench.py consumes CLI arguments at import; unittest's are unrelated.
    with patch.object(sys, 'argv', [str(SCRIPTS / 'bench.py')]):
        spec.loader.exec_module(bench)
    spec = importlib.util.spec_from_file_location('suss_bench_history', SCRIPTS / 'history.py')
    history = importlib.util.module_from_spec(spec)
    with patch.dict(sys.modules, {'bench': bench}):
        spec.loader.exec_module(history)
    return history


HISTORY = load_history()


class BenchHistoryTests(unittest.TestCase):
    def test_legacy_counts_keep_control_changes_out_of_gross_counts(self):
        # Published #447 examples: the control changes, while f0 gets cheaper.
        for run, net, gross in [('13', 29, 124), ('14', 62, 101), ('15', 52, 77)]:
            with self.subTest(run=run):
                rows = HISTORY.with_control(HISTORY.rows(RESULTS / f'run_m15_{run}.txt'))
                self.assertEqual(rows['`(f0)`'][2:4], (net, gross))

    def test_whole_loop_values_retain_their_published_nanosecond_unit(self):
        rows = HISTORY.rows(RESULTS / 'run_m15_17.txt')
        label = '`(loop [i 0 acc 0] (if (< i n) (recur (inc i) (+ acc i)) acc))`'
        row = rows[label]
        self.assertEqual(row[0], 'ns per iteration')
        self.assertEqual((row[1], row[2], row[4]), (1.1, 21, 0.84))
