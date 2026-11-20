# Raw integer predicate and direct parity conditional, 2026-11-20

Instruction-only comparisons use the unchanged seven bench.py programs, warm-up
first and median three N=1000-minus-0 measurements. They establish no walltime
or milestone acceptance. Generated bench.sus/bench.wit are byte-identical for
baseline and candidate; every raw perf observation and executable hash remains
in the instruction JSON files.

The baseline is b137ae4c's executable, whose Rust/core tree is identical to the
b676b991 documentation-only head. Candidate implementation `2ad0c15d` lowers
Bitwise::LongTest to the existing nominal long test, preserving logical Bool
while retaining a raw flag; even? inverts its checked raw low bit with a
conditional instead of calling not. The integer guard, errors and argument
order remain. Core changes are replacement JSON with regenerated core/manifest.
Shared ABI 7 and source-kind rules are unchanged.

| Program | Baseline instructions/run | Candidate | Reduction |
| --- | ---: | ---: | ---: |
| fib | 2,068,711.481 | 2,068,711.506 | -0.000001% |
| word-frequencies | 3,411,899.295 | 3,411,910.507 | -0.000329% |
| transduce | 764,570.015 | 677,006.787 | 11.452611% |
| lazy-pipeline | 1,146,971.197 | 1,059,416.537 | 7.633554% |
| into-vector | 1,659,619.457 | 1,659,615.439 | 0.000242% |
| into-map | 11,919,216.403 | 11,919,216.105 | 0.000003% |
| string-build | 1,361,065.832 | 1,361,058.356 | 0.000549% |

The predicate-only intermediate gives lazy-pipeline 1,113,163.634 and transduce
730,775.059 instructions/run; its full raw data is also retained.
parity-executable.json records the exact source patch used for the combined
executable, also checked byte-for-byte against the published implementation
commit. implementation.patch retains those bytes. Candidate executable SHA256:
`f4d8d30f791afdc440152b1806e757df2cfd77fff55c4b91ad1a7bdec991510a`.

Twenty-one focused tests across four CLI targets pass: clojure_equality_bitwise,
compiled_core_casts, compiled_primitive_params, compiled_call_sites. New integer
predicate matrices and boolean capture/branch/single-evaluation results match
fresh JVM Clojure 1.12.6 exactly (jvm-observations.txt). Existing parity cases
include wide longs, both long limits, noninteger rejection and effects. The 5x
walltime targets require a separate quiet full benchmark.

Java-free bootstrap verification, with stable pinned for nested Cargo calls,
reproduces both phase artifacts byte-for-byte, verifies compiler identities
and passes all four release bootstrap execution tests. After updating the
stacked base to6a91566f, all356 Python tests and core/bootstrap checks pass.

After incorporating main eb9942b1 (the merged #481 history report/fixtures),
all358 Python tests and core/bootstrap checks pass; compiler/runtime/bench.py
inputs remain byte-identical to implementation2ad0c15d.
