#!/usr/bin/env python3
"""Instructions per operation across M15 packages, from checked-in bench.py runs.

Each package that changes performance adds its `bench.py` output under
`results/` and a line to RUNS; this prints the tables that
`results/m15_history.md` holds. Instruction counts don't depend on machine
load, so columns from different days compare; the JVM column is the latest
run's.

The first table counts each case with its control loop, so a change to a
control doesn't show as a change to every case (#447); the second, less it.

Usage: history.py > experiments/numeric-representation/results/m15_history.md
"""
import os

import bench

RESULTS = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "results")
# Package, commit and results file, oldest first.
RUNS = [
    ("M15-01 baseline", "5e5dbc53", "run_m15_baseline.txt"),
    ("M15-02 var reads", "28309f6d", "run_m15_02.txt"),
    ("M15-03 inline predicates", "168e4311", "run_m15_03.txt"),
    ("M15-04 structured control", "a35c5699", "run_m15_04.txt"),
    ("M15-05 unboxed locals, ADR-0004", "53958035", "run_m15_05.txt"),
    ("M15-06 calling convention", "e5daf0ff", "run_m15_06.txt"),
    ("M15-07 direct linking", "151c90ad", "run_m15_07.txt"),
    ("M15-08 dispatch entries", "c00a97c9", "run_m15_08a.txt"),
    ("M15-09 keyword interning, fast paths", "06783f4d", "run_m15_09.txt"),
    ("M15-10–12 Binaryen, vector-of, dense aset", "ebda997f", "run_m15_final.txt"),
    ("M15-13 string cache, hints", "d4ca6b92", "run_m15_13.txt"),
    ("M15-14 raw values, object arrays, wasmtime 51", "de6b1925", "run_m15_14.txt"),
    ("M15-15 Clojure arrays, loop-invariant var reads", "bacdb2fb", "run_m15_15.txt"),
    ("M15-16 array views, string literal globals", "6952319c", "run_m15_16.txt"),
]
# Each table names its columns in its header: runs since #386 have Suss bytes,
# since #443 JVM bytes, and since #447 "With control", the count with the
# control's. For older runs, that count is computed from the control's row.


def time(text):
    # A row's time, or None for one below the bench's resolution; runs before
    # the bench printed ≈ 0 could show negative times, which are below it too.
    value = None if text in ("≈ 0", "") else float(text.replace(",", ""))
    return value if value is not None and value > 0 else None


def number(text):
    return int(text.replace(",", "")) if text else None


def rows(path):
    # Label -> (section unit, Suss time, instructions, instructions with the
    # control, JVM time), in order; a time is None where it's below the
    # bench's resolution, and a count None where the run has none.
    section, columns, found = "ns per operation", {}, {}
    for line in open(path):
        line = line.strip()
        if line.startswith("Per element"):
            section = "ns per element"
        elif line.startswith("Programs"):
            section = "µs per run"
        if not line.startswith("| ") or line.startswith("| ---"):
            continue
        cells = [cell.strip() for cell in line[1:-1].split(" | ")]
        if cells[0] in ("Case", "Loop"):
            columns = {name.replace(" µs", " ns"): i for i, name in enumerate(cells)}
            continue

        def cell(name):
            return cells[columns[name]] if name in columns else ""

        found[cells[0]] = (section, time(cell("Suss ns")), number(cell("Suss instructions")),
                           number(cell("With control")), time(cell("JVM ns")))
    return found


def with_control(found):
    # Fills in the count with the control where a run predates that column,
    # from the case's control in bench.CASES. Other rows have no control, so
    # their count is the whole loop's already.
    expressions = {name: expr for name, expr, *_ in bench.CASES}
    for name, expr, control, _ in bench.CASES:
        label, control_label = bench.label(name, expr, control), bench.label(control, expressions.get(control), None)
        if label not in found or found[label][3] is not None:
            continue
        section, suss, count, _, jvm = found[label]
        if control and count is not None and control_label in found and found[control_label][2] is not None:
            found[label] = (section, suss, count, count + found[control_label][2], jvm)
    for label, (section, suss, count, total, jvm) in found.items():
        if total is None and not any(bench.label(n, e, c) == label for n, e, c, _ in bench.CASES if c):
            found[label] = (section, suss, count, count, jvm)
    return found


def history(runs, gross):
    # One table: each run's counts with the control (gross) or less it.
    latest = runs[-1][2]
    labels = list(latest) + [label for _, _, found in runs for label in found if label not in latest]
    columns = [f"{name} ({commit})" for name, commit, _ in runs]
    print("| Case | " + " | ".join(columns) + " | Suss | JVM |")
    print("| --- |" + " --- |" * (len(columns) + 2))
    section = None
    for label in labels:
        unit = next(found[label][0] for _, _, found in reversed(runs) if label in found)
        if unit != section:
            section = unit
            print(f"| **{unit}** |" + " |" * (len(columns) + 2))
        counts = []
        for _, _, found in runs:
            count = found.get(label, (None,) * 5)[3 if gross else 2]
            counts.append("—" if count is None else f"{count:,}")
        if label in latest:
            _, suss, _, _, jvm = latest[label]
            times = ("≈ 0" if suss is None else f"{suss:,.1f}") + " | " + \
                ("≈ 0" if jvm is None else f"{jvm:,.2f}")
        else:
            times = "— | —"
        print(f"| {label} | " + " | ".join(counts) + f" | {times} |")


def main():
    runs = [(name, commit, with_control(rows(os.path.join(RESULTS, path)))) for name, commit, path in RUNS]
    print("# M15 performance history\n")
    print("Generated by `suss-jvm/history.py` from `bench.py` runs; do not edit by hand.")
    print("User-space instructions per operation for each M15 package (#349). The")
    print("last columns are the latest run's Suss and JVM Clojure 1.12.6 times, less")
    print("the control loop's.\n")
    print("## With the control\n")
    print("Each case's whole loop iteration, its control's instructions included, so")
    print("a change to a control doesn't move the other counts (#447).\n")
    history(runs, gross=True)
    print("\n## Less the control\n")
    print("Each case's count less its control loop's; a control's row, and the")
    print("reductions, programs and loops, which have none, are the whole loop")
    print("iteration.\n")
    history(runs, gross=False)


if __name__ == "__main__":
    main()
