# Complete core-environment diagnostic costs

Source: `c59b6237a2eb9fe6bf353304cd1efe0ac2e40901` with only the original
CLI fixture's Macro operation allowance changed for diagnostics. Production
source, bootstrap artifacts, caller allowances and all original assertions are
unchanged. Controllers retain input hashes, process-group drain and exact owned
file restoration. Both runs are terminal and restored; independent reviews are
included.

The 2B diagnostic completes the first Runtime-caller expansion, then exhausts
fuel during the second caller environment at source0..36. It reports0PASS/1FAIL;
the Macro-caller phase is unexecuted. The 10B diagnostic executes the entire
fixture:1PASS/0FAIL/0ignored, both caller phases, both projections, collection
checks and every original assertion. Test duration694.26seconds; controller
stage696.43seconds includes command overhead. These are diagnostic results,
not default-budget acceptance or a measurement of complete compilation fuel.

| Caller | Declaration | Complete traced expansion | Caller environment |
|---|---|---:|---:|
| Runtime | identity | 1,299,517,125 | 1,299,297,301 |
| Runtime | graph-owned | 3,766,083,473 | 3,765,857,427 |
| Macro | identity | 1,299,585,605 | 1,299,365,781 |
| Macro | graph-owned | 3,766,081,893 | 3,765,855,847 |

Expansion totals subtract the decoded checkpoint from the start checkpoint;
environment totals subtract caller-environment from caller-form. Work outside
these checkpoints is excluded. The maximum observed expansion is3,766,083,473;
10B is an experimental cap, not a calibrated default or a proven minimum.

The first Runtime expansion matches the 2B run exactly. Compared with the
preceding ad6 completed first expansion of1,706,700,367, it saves407,183,242
units (23.8579%). This measures fuel for that expansion, not elapsed-time or
whole-program improvement. No complete ad6 second expansion exists for comparison.

## Remaining cost attribution

An independently reviewed partition of the historical ad6 second-request
*completed prefix* finds658 complete validation checks costing25,261,472 units.
Between the first validation start and last completed prototypes checkpoint,
1,832,406,256 units fall in unlabelled inter-check gaps. This does not assign
those gaps to a single function or establish c59 counts. One fresh-start marker
has no completed recording; the trace does not support wholesale cache rebuilding.

The next measurement should separate native expectation projection/witness
capture, the current-source logical predicate and outer materialization. A small
constructor getter optimization is not supported as the dominant target by this
trace. All integrity checks, metadata, evaluation order and host charges remain
required.

## Reproduction and limits

The archived manifests specify exact selectors and environments. Both use:

```sh
CARGO_BUILD_JOBS=1 RAYON_NUM_THREADS=1 \
CARGO_TARGET_DIR=/home/bobby/.cache/suss-m4-work/m4-cost-diagnostic-target \
SUSS_MACRO_FUEL_TRACE=1 \
cargo test --locked -p suss-cli --test compiled_macro_core_environment \
  compiled_macro_core_namespace_keeps_complete_declaration_records \
  -- --exact --test-threads=1 --nocapture
```

Reproduction requires applying the archived test-only allowance overlay. The
normal fixture retains its original allowance. Issue252's no-increase acceptance
criterion remains unchanged; a compilation-budget policy revision requires an
explicit decision. Exhaustion/cumulative-accounting tests remain independent of
any future integration-budget calibration.

Exact source-head CI [39172955939](https://github.com/bobby/suss/actions/runs/39172955939)
is terminal failure: foundation succeeds; integration shards fail. No passing
full baseline, readiness or issue completion is claimed. Do not infer complete
suite counts from the job verdicts. The required full baseline remains
`cargo test --workspace --locked -- --test-threads=2` after focused blockers pass.

[Raw diagnostic evidence](m4-pr256-complete-coreenv-cost-2026-10-18.tar.gz)
SHA256: `c3f328b7a949450327c9e22fd652af90b90d8bacc303da29f5f43b57e2762afe`. Archive members are regular relative paths; SHA256SUMS.json
binds all other payloads, including the raw logs and source freezes.
