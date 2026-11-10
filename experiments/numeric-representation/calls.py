#!/usr/bin/env python3
"""User-space instructions per call for each calls.wat kernel, on wasmtime
(target/release/calls) and, with --node, on V8 (calls.mjs): (count at n) -
(count at 0), over n, less the inline kernel. Instruction counts don't depend
on machine load.

Usage: calls.py [n] [--node]
"""
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
KERNELS = ["inline", "direct", "indirect", "indirect_typed", "table_ref",
           "field_typed", "field_untyped", "index_final", "index_subtype", "proposed", "today"]
N = int(next((a for a in sys.argv[1:] if a.isdigit()), 20_000_000))
NODE = "--node" in sys.argv


def command(kernel, n):
    if NODE:
        return ["node", "--single-threaded", "--no-liftoff", os.path.join(HERE, "calls.mjs"), os.path.join(HERE, "target", "calls.wasm"),
                str(n), "1", kernel]
    return [os.path.join(HERE, "target", "release", "calls"), str(n), "1", kernel]


def instructions(kernel, n):
    out = subprocess.run(["perf", "stat", "-x,", "-e", "instructions:u", *command(kernel, n)],
                         capture_output=True, text=True, check=True).stderr
    return min(int(line.split(",")[0]) for line in out.splitlines()
               if line.split(",")[0].isdigit())


per_call = {}
for kernel in KERNELS:
    zero = min(instructions(kernel, 0) for _ in range(3))
    full = min(instructions(kernel, N) for _ in range(3))
    per_call[kernel] = (full - zero) / N
for kernel in KERNELS:
    extra = per_call[kernel] - per_call["inline"]
    print(f"{kernel:16} {per_call[kernel]:7.1f} instructions/iteration, {extra:6.1f} for the call")
