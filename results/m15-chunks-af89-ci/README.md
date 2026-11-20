# Paired quiet benchmark, 2026-11-20

[CI run 44756810859](https://github.com/bobby/suss/actions/runs/44756810859)
measured main `33fd175afb7d387955ff9acd322ccd3b3cb57334` and candidate
`af89e33e9c8339ec41b36d4d97874017fe691639` serially on one fresh runner.
The benchmark source is unchanged, n=5,000,000, reps=3. Each complete run
followed its own standalone load <=0.75 for 60 seconds idle gate. JVM Clojure
1.12.6 ran on Temurin 21. Load peaked at 1.625 for baseline and 1.901 for
candidate. The receipts retain every load sample and executable/source hash.
The runner refused hardware counters; these timing results contain no invented
instruction measurements.

| Program | Baseline Suss (microseconds/run) | Candidate Suss | Candidate JVM | Candidate ratio |
| --- | ---: | ---: | ---: | ---: |
| fib | 78.717 | 77.671 | 80.448 | 0.965480 |
| word-frequencies | 199.155 | 198.576 | 38.558 | 5.150063 |
| transduce | 41.792 | 33.313 | 5.982 | 5.569036 |
| lazy-pipeline | 139.422 | 64.609 | 11.808 | 5.471700 |
| into-vector | 71.642 | 72.510 | 11.828 | 6.130349 |
| into-map | 803.135 | 802.642 | 54.901 | 14.619764 |
| string-build | 225.420 | 69.225 | 28.893 | 2.395912 |

No program regressed beyond the sum of the two observed spreads. String-build
allocated 61,440 bytes/run, below 200,000. The original combined gate **failed**
because lazy-pipeline exceeded 5x; its unmodified result is
`combined-comparison.json`. Re-evaluating the same data for issue #450 alone
produces `string-comparison.json`, which also explicitly lists all remaining
programs above 5x. This does not establish issue #482 or milestone acceptance.

Reproduce either comparison from this directory's parent repository root:

```sh
systemd-run --user --scope -p MemoryMax=512M python3 scripts/performance_bench.py compare \
  --baseline results/m15-chunks-af89-ci/baseline \
  --candidate results/m15-chunks-af89-ci/candidate \
  --output target/probe/combined-comparison.json
# The command above must fail while lazy-pipeline exceeds 5x.
systemd-run --user --scope -p MemoryMax=512M python3 scripts/performance_bench.py compare \
  --baseline results/m15-chunks-af89-ci/baseline \
  --candidate results/m15-chunks-af89-ci/candidate \
  --require-program string-build --output target/probe/string-comparison.json
```

The complete CI artifact, including both frozen executables, is
`quiet-performance-af89e33e9c8339ec41b36d4d97874017fe691639` (artifact ID
14643165006). Candidate executable SHA256 is
`f24d2606fd1523e77c02d5ffeaa576f9d0176090ee76e55cd1d93cddddbcab75`.
`baseline.txt` and `candidate.txt` retain the complete benchmark console output;
the runtime JSON files contain unrounded medians, spreads and allocations.

## Exact-executable output and scaling

The downloaded candidate executable also ran local output and instruction-only
probes in bounded user scopes. These establish no additional walltime result.
`string-output.sus` and `string-output-jvm.clj` produce the complete output for
1,024 and 10,240 numeric elements. The outer console whitespace was stripped
before comparing the actual full printed vectors; they match exactly, 43,098
UTF-8 bytes, SHA256
`bc7d4219264892d6bd713a76c28f03f10c43f70f50498efcc292095e38bd23b1`.
`string-output.txt` preserves that full result. The output probe uses compile
fuel 100,000,000 and no optional runtime fuel cap; shipped defaults are unchanged.

`linearity.sus` and `linearity.wit` are the actual instruction-probe sources.
`linearity-instructions.json` retains all perf observations, with a warm-up
case first and median three N=1000-minus-0 measurements. Numeric elements cost
1,373,808.957 versus 14,295,679.089 instructions, a **10.405871x** increase for
10x input; fixed-width strings cost 1,264,327.821 versus 12,913,105.156,
**10.213415x**. `output-and-linearity.json` identifies the exact executable and
records both comparisons. These approximately linear counts include the
increased digit widths of numeric values.
