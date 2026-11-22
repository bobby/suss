# Numeric box reuse evidence

A lazy GC copy is reused across blocks until its raw SSA definition executes again.
Only potential reuse adds a cache flag. The initial broad-flag version is retained
because it increased boxed-control instruction counts; selective flags remove that cost.

| Case | Before bytes/iteration | Selective bytes/iteration | Before instructions | Selective instructions |
| --- | ---: | ---: | ---: | ---: |
| double-raw | 13.496 | ≈0 | 103.833 | 96.101 |
| double-boxed | ≈0 | ≈0 | 115.918 | 116.049 |
| long-raw | 13.355 | ≈0 | 100.057 | 91.984 |
| long-boxed | ≈0 | ≈0 | 112.998 | 113.038 |

Raw samples, scripts, exact executable/component hashes and component byte sizes
are in each version directory and receipts.json. These are RSS slope estimates
under the null collector, following bench.py, not exact object counts. Measurements
ran alongside builds; wall times are retained and are not quiet timing acceptance.
The benchmark includes both boxed controls; their movement is not hidden by subtraction.

The final backend passes47 focused tests across nine targets. The extended cache
regression adds multiple escaping uses of changing loop values and cross-block NaN
keys/equality; both Runtime/Macro and actual AOT pass. Three value cases match fresh
JVM1.12.6. The fourth follows the existing Suss numeric identity contract, verified
on the frozen before binary. The initial wrong JVM identity expectation is retained.
NaN observations retain the separate JVM boxed-identity difference. No reference
or known-failure baseline is rewritten. All361 Python tests pass; the four new
bench.py worker cases each match JVM results at0/1/7/33.

The code-probe validates complete components and hashes each marked function body.
Before and selective versions have identical309-byte and380-byte checked sum bodies;
both exports execute correctly. The generated no-allocation/call checks also pass.
This preserves the checked sum-loop code behind the existing kernel comparison;
a new quiet timing ratio is not inferred from these concurrent samples.

Reproduce the measurements by building the compiler parent named in receipts.json
and applying the retained source patch for each variant. Compile its bench.sus with
bench.wit and the four explicit exports; each measure.py records warm runs and all
samples. The scripts name local frozen executables; point those paths at the
corresponding rebuilt binaries. Build the independent code-probe with its locked
manifest and pass both sum components to compare their body hashes.

The compiler parent and patches identify the measured sources before the later
fixture-only stack rebase. No measurements are relabeled as final-head timings.
This is partial evidence for #337/#351. Remaining numeric formal overloads and
nonescaping collections/closures are outside this measured change.
