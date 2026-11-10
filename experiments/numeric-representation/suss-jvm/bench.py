#!/usr/bin/env python3
"""Operations and small programs in compiled Suss against JVM Clojure (#336, #337, #347).

Each case is one expression evaluated in a loop of `n` iterations; the cost
per operation is the loop's time less the time of its control loop, which
evaluates only the operands, divided by the number of operations. Suss runs
compiled components with `suss run --invoke` (no fuel metering); the JVM side
runs the same forms in one process, timed with System/nanoTime after warm-up.
Each JVM iteration stores its result in a sink array, so the JIT can't drop the
operation; the control loop stores too, so the store cancels out.

Where `perf` is installed, each Suss case also reports user-space
instructions per operation from `perf stat -e instructions:u`: the count at n
less the count at 0, which unlike time doesn't depend on machine load.

Usage: bench.py [n] [reps]   (from the repository root, after building a
release suss)
"""
import os
import shutil
import subprocess
import sys
import tempfile
import time

N = int(sys.argv[1]) if len(sys.argv) > 1 else 5_000_000
REPS = int(sys.argv[2]) if len(sys.argv) > 2 else 3
SUSS = os.path.abspath("target/release/suss")
PERF = shutil.which("perf")

# The loop's operands: `i` is the loop counter, a long, and `m` is it masked
# to an index. Arrays have 1024 elements. JVM forms add the type hints
# idiomatic Clojure code would use, so reflection doesn't dominate.
SETUP = [
    ("la", "(long-array (range 1024))", "^longs"),
    ("da", "(double-array (range 1024))", "^doubles"),
    ("ia", "(int-array (range 1024))", "^ints"),
    ("oa", "(to-array (range 1024))", "^objects"),
    ("v", "(vec (range 1024))", ""),
    ("m", "(zipmap (range 1024) (range 1024))", ""),
    ("km", "{:a 1 :b 2 :c 3 :d 4}", ""),
    ("s", "(set (range 1024))", ""),
    ("words", "(vec (map (fn [i] (keyword (str \"w\" (mod i 37)))) (range 1024)))", ""),
]
# Definitions shared by both sides: for the call cases (#349), functions of
# each arity, a dynamic var, a protocol implemented by a type and a
# multimethod; and for the programs, a recursive function.
DEFS = [
    "(defn f0 [] 1)",
    "(defn f1 [x] x)",
    "(defn f2 [x y] x)",
    "(defn f3 [x y z] x)",
    "(defn fv [x & more] x)",
    "(def ^:dynamic *d* 0)",
    "(defprotocol Ident (ident [x]))",
    "(deftype Box [] Ident (ident [_] 1))",
    "(def b (->Box))",
    "(defmulti area :shape)",
    "(defmethod area :square [x] 1)",
    "(def sq {:shape :square})",
    "(defn fib [n] (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))",
]
# Forms around a case's loop, for cases that need a local or bindings.
WRAP = {
    "call-local": "(let [g (fn [x] x)] {})",
    "call-bound": "(binding [*d* 1] (binding [*d* 2] (binding [*d* 3] {})))",
}
# name, expression, control, and the divisor of n for its loop count: object
# arrays, which Suss holds as a chain of entries above a small length
# (sparse_arrays.rs), take microseconds per access.
CASES = [
    ("long-control", "i", None, 1),
    # A call to a one-argument core function, and an inline comparison, for
    # what `=` and `hash` cost beyond the call.
    ("long-identity", "(identity i)", "long-control", 1),
    # Calls (#349): user functions of each arity, a variadic one, a local
    # closure, a core function inside three `binding`s, a protocol method,
    # `apply` and a multimethod.
    ("call-0", "(f0)", "long-control", 1),
    ("call-1", "(f1 i)", "long-control", 1),
    ("call-2", "(f2 i i)", "long-control", 1),
    ("call-3", "(f3 i i i)", "long-control", 1),
    ("call-variadic", "(fv i)", "long-control", 1),
    ("call-local", "(g i)", "long-control", 1),
    ("call-bound", "(identity i)", "long-control", 1),
    ("call-protocol", "(ident b)", "long-control", 1),
    ("call-apply", "(apply f2 i [i])", "long-control", 1),
    ("call-multimethod", "(area sq)", "long-control", 1),
    ("long-identical", "(identical? i 7)", "long-control", 1),
    ("long-nil", "(nil? i)", "long-control", 1),
    ("keyword-eq", "(= :a :b)", "long-control", 1),
    ("long-eq", "(= i 7)", "long-control", 1),
    ("long-eq-double", "(= i 7.5)", "long-control", 1),
    ("long-num-eq", "(== i 7)", "long-control", 1),
    ("long-add", "(+ i 7)", "long-control", 1),
    ("long-hash", "(hash i)", "long-control", 1),
    ("long-str", "(str \"a\" i)", "long-control", 1),
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
    # Persistent collections of 1024 entries, and a small keyword map.
    ("vector-conj", "(conj v i)", "long-control", 1),
    ("map-get", "(get m (bit-and i 1023))", "index-control", 1),
    ("map-assoc", "(assoc m (bit-and i 1023) i)", "index-control", 1),
    ("keyword-get", "(:b km)", "long-control", 1),
    ("set-contains", "(contains? s (bit-and i 1023))", "index-control", 1),
]
# Whole-collection reductions, 1024 elements each, run n/1024/divisor times.
REDUCTIONS = [
    ("long-array-reduce", "(reduce + la)", 1),
    ("double-array-reduce", "(reduce + da)", 1),
    ("object-array-reduce", "(reduce + oa)", 100),
    ("vector-reduce", "(reduce + v)", 1),
]
# Small programs, run n/100,000 times: 50 runs at the default n.
PROGRAMS = [
    ("fib", "(fib 20)"),
    ("word-frequencies", "(reduce (fn [acc w] (assoc acc w (inc (get acc w 0)))) {} words)"),
    ("transduce", "(transduce (comp (map inc) (filter even?)) + 0 v)"),
    ("lazy-pipeline", "(reduce + (map inc (filter even? v)))"),
    ("into-vector", "(into [] (map inc) v)"),
    ("into-map", "(into {} (map (fn [x] [x x])) v)"),
    ("string-build", "(apply str (map str v))"),
]
PROGRAM_DIVISOR = 100_000

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


def loop(name, expr, n, sink=False):
    # The result feeds the accumulator, or on the JVM the sink, so neither side
    # can drop the operation.
    use = "(do (sink! r) (recur (inc i) c))" if sink else "(recur (inc i) (if (identical? r nil) (inc c) c))"
    form = f"(loop [i 0 c 0] (if (< i {n}) (let [r {expr}] {use}) c))"
    return WRAP.get(name, "{}").format(form)


def hinted(expr):
    expr = JVM.get(expr, expr)
    for name, _, hint in SETUP:
        if hint:
            expr = expr.replace(f" {name} ", f" {hint} {name} ").replace(f" {name})", f" {hint} {name})")
    return expr


def counted():
    # (name, expression, loop count) of every case.
    return ([(name, expr, N // divisor) for name, expr, _, divisor in CASES]
            + [(name, expr, N // 1024 // divisor) for name, expr, divisor in REDUCTIONS]
            + [(name, expr, N // PROGRAM_DIVISOR) for name, expr in PROGRAMS])


def suss_times(work):
    cases = counted()
    source = ["(ns bench)"] + [f"(def {name} {init})" for name, init, _ in SETUP] + DEFS
    for name, expr, _ in cases:
        source.append(f"(def case-{name} (fn [n] {loop(name, expr, 'n')}))")
    for name, body, _ in LOOPS:
        source.append(f"(def case-{name} (fn [n] {body}))")
    with open(os.path.join(work, "bench.sus"), "w") as file:
        file.write("\n".join(source) + "\n")
    with open(os.path.join(work, "bench.wit"), "w") as file:
        exports = " ".join(f"export case-{name}: func(n: u32) -> u32;" for name, *_ in cases)
        exports += "".join(f" export case-{name}: func(n: u32) -> {ty};" for name, _, ty in LOOPS)
        file.write(f"package bench:numeric; world bench {{ {exports} }}\n")
    exports = [arg for name, *_ in cases + LOOPS for arg in ("--export", f"case-{name}=bench/case-{name}")]
    compiled = subprocess.run(
        [SUSS, "--build-dir", os.path.join(work, "build"), "compile", "bench.sus",
         "-w", "bench.wit", "-o", "bench.wasm", *exports],
        cwd=work, capture_output=True, text=True,
    )
    if compiled.returncode:
        sys.exit(compiled.stderr)

    def run(name, n):
        # Seconds and user-space instructions (None without perf) of one run.
        command = [SUSS, "run", "bench.wasm", "--invoke", f"case-{name}", "--", str(n)]
        if PERF:
            command = [PERF, "stat", "-x,", "-e", "instructions:u", "--", *command]
        start = time.perf_counter()
        output = subprocess.run(command, cwd=work, check=True, capture_output=True, text=True)
        seconds = time.perf_counter() - start
        counts = [int(line.split(",")[0]) for line in output.stderr.splitlines()
                  if "instructions" in line and line.split(",")[0].isdigit()]
        return seconds, (counts[0] if counts else None)

    def best(name, n):
        runs = [run(name, n) for _ in range(REPS)]
        return min(s for s, _ in runs), (min(c for _, c in runs) if PERF else None)

    run("long-control", 1)  # Compiles and caches native code.
    # Seconds per iteration, less process startup, and instructions per
    # iteration, less the case's count at 0.
    startup = best("long-control", 0)[0]
    times, instructions = {"startup": startup}, {}
    for name, _, count in cases + [(name, body, N) for name, body, _ in LOOPS]:
        seconds, total = best(name, count)
        times[name] = (seconds - startup) / count
        if PERF:
            instructions[name] = (total - best(name, 0)[1]) / count
    return times, instructions


def jvm_times(work):
    lines = ["(set! *warn-on-reflection* true)"]
    lines += [f"(def {name} {init})" for name, init, _ in SETUP] + DEFS
    lines.append("(def sink-array (object-array 1))")
    lines.append("(defn sink! [x] (aset ^objects sink-array 0 x))")
    lines.append("(defmacro timed [n expr] `(fn [] (let [t# (System/nanoTime)] "
                 "(~expr ~n) (/ (- (System/nanoTime) t#) 1e9))))")
    entries = []
    for name, expr, count in counted():
        lines.append(f"(defn case-{name} [^long n] {loop(name, hinted(expr), 'n', sink=True)})")
        entries.append(f'["{name}" {count} (timed {count} case-{name})]')
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
                            cwd="/tmp", capture_output=True, text=True)
    if output.returncode:
        sys.exit(output.stderr[-3000:])
    reflection = [line for line in output.stderr.splitlines() if "Reflection warning" in line]
    if reflection:
        sys.exit("\n".join(reflection))
    return {name: float(value) for name, value in (line.split() for line in output.stdout.splitlines())}


def table(suss, instructions, jvm):
    def per_op(values, name, control, scale):
        return (values[name] - (values[control] if control else 0.0)) * scale

    def row(label, s, i, j):
        ratio = f"{s / j:.0f}×" if j >= 0.05 and s > 0 else ""
        count = "" if i is None else f"{i:,.0f}"
        return f"| {label} | {s:,.1f} | {count} | {j:,.2f} | {ratio} |"

    header = ["| Case | Suss ns | Suss instructions | JVM ns | Ratio |", "| --- | --- | --- | --- | --- |"]
    print(f"n = {N}, best of {REPS}; Suss startup {suss['startup']:.2f} s\n")
    print("Nanoseconds and user-space instructions per operation, less its control")
    print("loop; a control's row is the whole loop iteration.\n")
    print("\n".join(header))
    for name, expr, control, _ in CASES:
        label = f"`{expr}`" + ("" if control else " (control)")
        if name in WRAP:
            label += f" in `{WRAP[name].format('…')}`"
        count = per_op(instructions, name, control, 1) if instructions else None
        print(row(label, per_op(suss, name, control, 1e9), count, per_op(jvm, name, control, 1e9)))
    print("\nPer element of 1024, the loop step included:\n")
    print("\n".join(header))
    for name, expr, _ in REDUCTIONS:
        count = instructions[name] / 1024 if instructions else None
        print(row(f"`{expr}`", suss[name] / 1024 * 1e9, count, jvm[name] / 1024 * 1e9))
    print("\nPrograms, microseconds and instructions per run, the loop step included:\n")
    print("\n".join(h.replace(" ns", " µs") for h in header))
    for name, expr in PROGRAMS:
        count = instructions[name] if instructions else None
        print(row(f"{name}: `{expr}`", suss[name] * 1e6, count, jvm[name] * 1e6))
    print("\nWhole loops, per iteration:\n")
    print("| Loop | Suss ns | Suss instructions | JVM ns | Ratio |")
    print("| --- | --- | --- | --- | --- |")
    for name, body, _ in LOOPS:
        count = instructions[name] if instructions else None
        print(row(f"`{body}`", suss[name] * 1e9, count, jvm[name] * 1e9))


if __name__ == "__main__":
    work = tempfile.mkdtemp(prefix="suss-numeric-bench-")
    try:
        jvm = jvm_times(work)
        suss, instructions = suss_times(work)
        table(suss, instructions, jvm)
    finally:
        shutil.rmtree(work)
