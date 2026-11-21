# Printer buffer measurements

The printer now prepends already-converted strings to immutable chunks and
joins them once. Append costs constant storage work plus argument conversion;
reversal, array construction and the final UTF-16 copy are linear in the writes
and output. The complete retained printer functions otherwise keep their order,
options and writer calls. The general Closure StringBuffer API remains separate.
The private writer preserves conversion before assignment and captures its old
prefix before reentrant conversion, as the retained Apache-2.0 adapter does.

Actual release commands use `read_string_bench -- 80 --print` and `-- 160 --print`.
The example retains its numeric count result and reports actual printed units;
count inspection and reporting happen outside the timed interval.

| Roots | Output UTF-16 units | Before pr-str | Initial after pr-str |
| --- | ---: | ---: | ---: |
| 80 | 509,474 | 25.670 s | 0.545 s |
| 160 | 1,026,955 | 359.376 s | 0.999 s |

Output grows 2.016x and the initial after time grows 1.833x. Both sizes use the
same 3 GiB process bound and best-of-three policy with a count-text baseline.
These are observed wall times, including scheduler, GC and host effects; no
exact asymptotic exponent is fitted. All observations remain in `after.json`:
a repeat with full workspace compilation starting between sizes measured
0.417/1.474s, and a pair pinned to CPU 0 with owned background work excluded
from sibling CPUs 0/1 measured 0.912/0.972s. The compilation-overlap pair is not
a controlled scaling comparison. No slow observation is discarded. Background
thread affinities were restored after the paired run.

`outputs.json` verifies the complete before/after byte streams match at both
sizes, not just their counts. Deterministic gzip files retain the actual UTF-16
little-endian output without a BOM. Their uncompressed SHA256 values are:

- 80: `91111eaad5bd7a5b7a06a1b3682c21c179d371e8c8185ed2dcc96c10820a4c1d`
- 160: `28866f6ff8c3179e81be6bcdeaa70c72fb6c2800106e2a4923d35efae026a776`

The output-only probe uses the unchanged benchmark node generator and actual
read-string/pr-str pipeline. It collects GC before decoding the retained result.
Temporarily add `output_probe.rs` as the CLI example `m11_print_output` and invoke
it with `<roots> <UTF-16-output-path>` to reproduce. Exact executable/harness and
source identities are in the JSON receipts. The output-only executions are
correctness comparisons, not timing samples.

33 focused cases pass across collection_printing, object_printing,
portable_string_fields, printer_options and printing_chunks. They include
component execution, options, escaped Unicode/empty chunks/GC ownership, nested
printing and writer-conversion reentry. The initial strict-string prototype's
reentry failure is retained; frozen before execution produces `oldx!`, which the
corrected code reproduces in both phases. All 358 Python checks pass again.
The inherited record requests still fit 4,000,000 fuel, with environment costs
982,377 / 991,474 after the new core declarations. No default is raised.

A full workspace command with two test threads and actual Binaryen is running
separately; the focused receipts are not a terminal full-workspace result.
