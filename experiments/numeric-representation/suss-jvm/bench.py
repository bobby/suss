#!/usr/bin/env python3
"""Numeric operations in compiled Suss against JVM Clojure (#336, #337, #347).

Each case is one expression evaluated in a loop of `n` iterations; the cost
per operation is the loop's time less the time of its control loop, which
evaluates only the operands, divided by the number of operations. Suss runs
compiled components with `suss run --invoke` (no fuel metering); the JVM side
runs the same forms in one process, timed with System/nanoTime after warm-up.

Usage: bench.py [n] [reps]   (from the repository root, after building suss)
"""
import os
import shutil
import subprocess
import sys
import tempfile
import time

N = int(sys.argv[1]) if len(sys.argv) > 1 else 5_000_000
REPS = int(sys.argv[2]) if len(sys.argv) > 2 else 3
SUSS = os.path.abspath("target/debug/suss")

# The loop's operands: `i` is the loop counter, a long, and `m` is it masked
# to an index. Arrays have 1024 elements. JVM forms add the type hints
# idiomatic Clojure code would use, so reflection doesn't dominate.
SETUP = [
    ("la", "(long-array (range 1024))", "^longs"),
    ("da", "(double-array (range 1024))", "^doubles"),
    ("ia", "(int-array (range 1024))", "^ints"),
    ("oa", "(to-array (range 1024))", "^objects"),
    ("v", "(vec (range 1024))", ""),
]
# name, expression, control, and the divisor of n for its loop count: object
# arrays, which Suss holds as a chain of entries above a small length
# (sparse_arrays.rs), take microseconds per access.
CASES = [
    ("long-control", "i", None, 1),
    # A call to a one-argument core function, and an inline comparison, for
    # what `=` and `hash` cost beyond the call.
    ("long-identity", "(identity i)", "long-control", 1),
    ("long-identical", "(identical? i 7)", "long-control", 1),
    ("long-eq", "(= i 7)", "long-control", 1),
    ("long-eq-double", "(= i 7.5)", "long-control", 1),
    ("long-num-eq", "(== i 7)", "long-control", 1),
    ("long-add", "(+ i 7)", "long-control", 1),
    ("long-hash", "(hash i)", "long-control", 1),
    ("double-control", "(double i)", None, 1),
    ("double-eq", "(= (double i) 7.5)", "double-control", 1),
    ("double-add", "(+ (double i) 0.5)", "double-control", 1),
    ("double-hash", "(hash (double i))", "double-control", 1),
    ("index-control", "(bit-and i 1023)", None, 1),
    ("long-aget", "(aget la (bit-and i 1023))", "index-control", 1),
    ("int-aget", "(aget ia (bit-and i 1023))", "index-control", 1),
    ("double-aget", "(aget da (bit-and i 1023))", "index-control", 1),
    ("object-aget", "(aget oa (bit-and i 1023))", "index-control", 100),
    ("vector-nth", "(nth v (bit-and i 1023))", "index-control", 1),
    ("long-aset", "(aset la (bit-and i 1023) i)", "index-control", 1),
    ("double-aset", "(aset da (bit-and i 1023) 1.5)", "index-control", 1),
    ("object-aset", "(aset oa (bit-and i 1023) i)", "index-control", 100),
]
# Whole-collection reductions, 1024 elements each, run n/1024/divisor times.
REDUCTIONS = [
    ("long-array-reduce", "(reduce + la)", 1),
    ("double-array-reduce", "(reduce + da)", 1),
    ("object-array-reduce", "(reduce + oa)", 100),
    ("vector-reduce", "(reduce + v)", 1),
]

# Whole loops, timed per iteration with nothing subtracted, against the
# unboxed i64 kernels in ../src/bench.wat (#337): name, loop, WIT result.
LOOPS = [
    ("long-sum", "(loop [i 0 acc 0] (if (< i n) (recur (inc i) (+ acc i)) acc))", "s64"),
    ("double-sum", "(loop [i 0 acc 0.0] (if (< i n) (recur (inc i) (+ acc 0.5)) acc))", "f64"),
]


# JVM forms where the shared one would reflect: Util.equiv has no (long,
# double) overload and RT.aset none for an Object[] and a long, so Clojure code
# boxes the long, as Suss always does.
JVM = {
    "(= i 7.5)": "(= (num i) 7.5)",
    "(aset oa (bit-and i 1023) i)": "(aset oa (bit-and i 1023) (num i))",
}


def loop(expr, n):
    # The result feeds the accumulator, so neither side can drop the operation.
    return (
        f"(loop [i 0 c 0] (if (< i {n}) (let [r {expr}] "
        f"(recur (inc i) (if (identical? r nil) (inc c) c))) c))"
    )


def hinted(expr):
    expr = JVM.get(expr, expr)
    for name, _, hint in SETUP:
        if hint:
            expr = expr.replace(f" {name} ", f" {hint} {name} ").replace(f" {name})", f" {hint} {name})")
    return expr


def suss_times(work):
    names = [f"case-{name}" for name, *_ in CASES] + [f"case-{name}" for name, *_ in REDUCTIONS]
    source = ["(ns bench)"] + [f"(def {name} {init})" for name, init, _ in SETUP]
    for name, expr, *_ in CASES:
        source.append(f"(def case-{name} (fn [n] {loop(expr, 'n')}))")
    for name, expr, _ in REDUCTIONS:
        source.append(f"(def case-{name} (fn [n] {loop(expr, 'n')}))")
    for name, body, _ in LOOPS:
        source.append(f"(def case-{name} (fn [n] {body}))")
    with open(os.path.join(work, "bench.sus"), "w") as file:
        file.write("\n".join(source) + "\n")
    with open(os.path.join(work, "bench.wit"), "w") as file:
        exports = " ".join(f"export {name}: func(n: u32) -> u32;" for name in names)
        exports += "".join(f" export case-{name}: func(n: u32) -> {ty};" for name, _, ty in LOOPS)
        names += [f"case-{name}" for name, *_ in LOOPS]
        file.write(f"package bench:numeric; world bench {{ {exports} }}\n")
    exports = [arg for name in names for arg in ("--export", f"{name}=bench/{name}")]
    compiled = subprocess.run(
        [SUSS, "--build-dir", os.path.join(work, "build"), "compile", "bench.sus",
         "-w", "bench.wit", "-o", "bench.wasm", *exports],
        cwd=work, capture_output=True, text=True,
    )
    if compiled.returncode:
        sys.exit(compiled.stderr)

    def run(name, n):
        start = time.perf_counter()
        subprocess.run([SUSS, "run", "bench.wasm", "--invoke", name, "--", str(n)],
                       cwd=work, check=True, capture_output=True)
        return time.perf_counter() - start

    run(names[0], 1)  # Compiles and caches native code.
    times = {}
    # Seconds per iteration, less process startup and compilation.
    startup = min(run("case-long-control", 0) for _ in range(REPS))
    times = {"startup": startup}
    for name, _, _, divisor in CASES:
        count = N // divisor
        times[name] = (min(run(f"case-{name}", count) for _ in range(REPS)) - startup) / count
    for name, _, divisor in REDUCTIONS:
        count = N // 1024 // divisor
        times[name] = (min(run(f"case-{name}", count) for _ in range(REPS)) - startup) / count
    for name, *_ in LOOPS:
        times[name] = (min(run(f"case-{name}", N) for _ in range(REPS)) - startup) / N
    return times


def jvm_times(work):
    lines = ["(set! *warn-on-reflection* true)"]
    lines += [f"(def {name} {init})" for name, init, _ in SETUP]
    lines.append("(defmacro timed [n expr] `(fn [] (let [t# (System/nanoTime)] "
                 "(~expr ~n) (/ (- (System/nanoTime) t#) 1e9))))")
    entries = []
    for name, expr, _, divisor in CASES:
        lines.append(f"(defn case-{name} [^long n] {loop(hinted(expr), 'n')})")
        entries.append(f'["{name}" {N // divisor} (timed {N // divisor} case-{name})]')
    for name, expr, divisor in REDUCTIONS:
        lines.append(f"(defn case-{name} [^long n] {loop(hinted(expr), 'n')})")
        entries.append(f'["{name}" {N // 1024 // divisor} (timed {N // 1024 // divisor} case-{name})]')
    for name, body, _ in LOOPS:
        lines.append(f"(defn case-{name} [^long n] {body})")
        entries.append(f'["{name}" {N} (timed {N} case-{name})]')
    lines.append(f"(def cases [{' '.join(entries)}])")
    # Warm up every case, then take the best of REPS runs.
    lines.append("(doseq [[_ _ f] cases] (dotimes [_ 3] (f)))")
    # Seconds per iteration.
    lines.append(f"(doseq [[name count f] cases] (println name (/ (apply min (repeatedly {REPS} f)) count)))")
    path = os.path.join(work, "bench.clj")
    with open(path, "w") as file:
        file.write("\n".join(lines) + "\n")
    output = subprocess.run(["mise", "exec", "--", "clojure", "-M", path],
                            cwd="/tmp", check=True, capture_output=True, text=True)
    reflection = [line for line in output.stderr.splitlines() if "Reflection warning" in line]
    if reflection:
        sys.exit("\n".join(reflection))
    return {name: float(value) for name, value in (line.split() for line in output.stdout.splitlines())}


def table(suss, jvm):
    def per_op(times, name, control):
        return (times[name] - (times[control] if control else 0.0)) * 1e9

    def row(label, s, j):
        ratio = f"{s / j:.0f}×" if j >= 0.05 and s > 0 else ""
        return f"| {label} | {s:.1f} | {j:.2f} | {ratio} |"

    print(f"n = {N}, best of {REPS}; Suss startup {suss['startup']:.2f} s\n")
    print("Nanoseconds per operation, less its control loop; a control's row is")
    print("the whole loop iteration.\n")
    print("| Case | Suss ns | JVM ns | Ratio |")
    print("| --- | --- | --- | --- |")
    for name, expr, control, _ in CASES:
        label = f"`{expr}`" + ("" if control else " (control)")
        print(row(label, per_op(suss, name, control), per_op(jvm, name, control)))
    print("\nPer element of 1024, the loop step included:\n")
    print("| Case | Suss ns | JVM ns | Ratio |")
    print("| --- | --- | --- | --- |")
    for name, expr, _ in REDUCTIONS:
        print(row(f"`{expr}`", suss[name] / 1024 * 1e9, jvm[name] / 1024 * 1e9))
    print("\nWhole loops, per iteration:\n")
    print("| Loop | Suss ns | JVM ns | Ratio |")
    print("| --- | --- | --- | --- |")
    for name, body, _ in LOOPS:
        print(row(f"`{body}`", suss[name] * 1e9, jvm[name] * 1e9))


if __name__ == "__main__":
    work = tempfile.mkdtemp(prefix="suss-numeric-bench-")
    try:
        jvm = jvm_times(work)
        suss = suss_times(work)
        table(suss, jvm)
    finally:
        shutil.rmtree(work)
