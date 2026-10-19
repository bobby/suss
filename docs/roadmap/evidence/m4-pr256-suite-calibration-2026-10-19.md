# Full upstream-suite compilation calibration and reviewed failure delta

Base source: `c0c9c45945b23a8c06889f2cf12af90d7e38b6d3`. The test-only change
imports the existing semantic compilation allowance and sets it only for
`check("suite")`, after the unchanged `Harness::new`. Corpus compilation uses
10,000,000,000 fuel instead of 200,000,000; Runtime operations, harness setup,
self-test fixtures and the separate pinned-map control retain 200,000,000.
Shipped defaults, metering rates, the oracle pin, vendored programs, source
metadata and assertion expectations are unchanged.

## Executed captures

Both runs used `CARGO_TARGET_DIR=/home/bobby/.cache/suss-m4-work/m4-cost-diagnostic-target`,
`CARGO_BUILD_JOBS=1` and `RAYON_NUM_THREADS=1`, without setting RUSTFLAGS or the
baseline WRITE environment variable:

```sh
SUSS_CLOJURE_TEST_SUITE_CAPTURE=<capture-directory> cargo test --locked -p suss-compile --test clojure_test_suite clojure_test_suite_matches_reviewed_baseline -- --exact --test-threads=1 --nocapture
```

| Actual compilation allowance | Host passes | Guest judged | Comparator failures | Not executed | Failed namespaces |
| --- | ---: | ---: | ---: | ---: | ---: |
| Original 200M | 1067 | 25 | 22 | 4740 | 186 |
| Calibrated 10B | 1144 | 25 | 22 | 4663 | 185 |

Both captures actually terminated 101 against the old reviewed failure map:
405.16s including compilation for the original run; 318.97s including compilation
(316.92s execution) for the calibrated run. These failures are preserved, not
relabelled successful executions. The second run reaches all 77 `contains?`
assertions without failure. All 1,092 formerly successful raw assertion records
compare equal between captures; their raw skip counters are identical.
The independent classifier also protects all 960 passing/guest IDs derivable
from the older checked-in baseline: zero failed or missing IDs.

## Reviewed known-failure maintenance

The old baseline retained transport-cap diagnostics which no longer describe
execution. The full reference, raw captures and exact existing comparator were
reviewed before changing it. Eight namespace entries change: `contains?`, `map`
and `update` now load; `doseq`, `drop-last`, `lazy-seq`, `merge` and `realized?`
reach their actual unresolved Runtime names (`doseq`, `drop-last`, `assert`,
`any?`, `promise`). Those five remain explicit failures. All other namespace/test
failures and the existing skip-mismatch records remain unchanged. No assertion
is deleted, skipped, shifted or credited merely because its namespace loads.

The newly reachable `clojure.core-test.update/test-update#48` stays explicitly
`kind:error`, alongside the existing `partial/test-partial#2` error. The two
assertion errors plus 20 existing skip mismatches account for the comparator's
22 failures. The remaining 4,663 blocked assertions are not passes. A successful
Rust baseline-comparison test means this exact failure map is reproduced;
it does not mean full language conformance.

## Actual update exception and follow-up

The pinned CLJS reference passes:

```clojure
(= (apply update [{:k 1} :k inc 1 2 3 4]) {:k 2})
```

The broad harness observes an opaque thrown value with the generic decoder
reason `Host error: Unsupported ABI2 value layout`; this alone does not identify
the exception. A separate actual CLI execution of the unchanged inner expression
exits 1 (`Uncaught language exception`, 30.117s). Wrapping that same expression
with `(try ... (catch :default e (ex-message e)))` exits 0 and returns
`"Wrong arity"` (28.960s). The current `inc` binding is fixed unary, while the
variadic `update` branch supplies extra arguments. This is a newly exposed
portable-core discrepancy, tracked in
[issue #28](https://github.com/bobby/suss/issues/28#issuecomment-6260403428).
Its appropriate CLJS arity semantics require a separate repair; this change does
not globally weaken arity checks or mark this assertion successful.

## Independent review and validation boundaries

Independent review confirms the exact inverse of the test change restores the
entire old test source; only the support import and corpus-only Macro setter are
new. The full raw delta review approves the explicit known-failure refresh,
protecting old successes and skips. Raw observations, terminal receipts,
classification scripts/results and direct CLI output are retained with this
report. Historical CI at the base source has foundation and shards 0, 3, 4
successful; shard 1's strict old-map mismatch is a real failure. Other shard
results are not inferred. The earlier base-source full workspace run was
explicitly cancelled with exit 143 after 53 completed passing tests, not a
completed baseline. Required fresh complete-target/workspace and committed-head
CI results must be reported separately from these captures.

Retained capture archive: [m4-pr256-suite-calibration-2026-10-19.tar.gz](m4-pr256-suite-calibration-2026-10-19.tar.gz), SHA-256 `a7dd899264eb82f0d66f57035697ca44b17f381f2a74522d88c32fbcf03ccd04`. Its manifest binds every payload.
