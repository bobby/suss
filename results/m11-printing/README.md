# Printer buffer measurements

The buffer-only measurements below precede the quote-string review correction.
Final corrected-source receipts and results follow at the end.

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

## Long-string review correction

Independent review of 79fecdcf identified one remaining quadratic loop:
quote-string copied its accumulated prefix once per UTF-16 unit. Commit
0242d5cc replaces that loop with one unit scan, appending unchanged runs and
escaped units to the same private buffer and joining once. The complete pinned
seven-entry escape table and source notices remain. The new actual guest-string
regression preserves long escaped text and unpaired surrogates after GC in both
Runtime and Macro phases.

Final-source receipts are in `quote-fix.json`. All 34 focused printing cases and
358 Python checks pass; both optimized-component tests execute with actual
Binaryen 133 and pass. Full final output at both nested-data sizes still matches
the before bytes and digests above. `m11-quote-outputs-driver.log` records the
captures; these are correctness probes, not timing samples.

| Mode | Roots or blocks | Printed UTF-16 units | Buffer-only before quoting fix | Final CPU-pinned |
| --- | ---: | ---: | ---: | ---: |
| Nested | 80 | 509,474 | see historical table | 0.560 s |
| Nested | 160 | 1,026,955 | see historical table | 1.162 s |
| Single escaped string | 80 | 122,882 | 2.450 s | 0.073 s |
| Single escaped string | 160 | 245,762 | 7.296 s | 0.231 s |

The final nested pair grows 2.075x in time for 2.016x output. Commands add
`--string` to the unchanged `--print` benchmark to exercise one large escaped
string. Each pair uses the release profile, best of three less count-text
baseline, and a 3 GiB process bound. Final quiet samples pin the benchmark to
CPU 0 and exclude owned background work from sibling CPUs 0/1; affinities were
restored afterward. All additional final-source samples remain visible:
0.512/1.941s nested and 0.415/0.367s single-string, captured while bounded work
including optimized verification ran. These timings include GC, scheduler and
host effects; neither long-string pair supports an exact exponent fit. The
source removes both prefix-copy loops; do not infer an exact exponent from
these small noisy samples.

The 79fecdcf local full run was stopped for this material source correction and
is incomplete. A replacement final-source workspace run is separate from the
focused logs. Independent re-review and exact final-head CI are required for
promotion. No local full-workspace pass is inferred from focused tests.
