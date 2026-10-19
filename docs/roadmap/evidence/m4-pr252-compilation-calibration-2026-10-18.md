# Calibrated compilation and AOT failure diagnostics

The compiler-only allowance adopted under #252 is 10B for the semantic corpora
listed in `docs/runtime/compilation-test-fuel.md`. Runtime limits, sources,
metadata and semantic assertions are retained. Deliberate default/exhaustion
controls retain their original limits. Shipped defaults remain10M.

## Published calibration source6d51c85

With CARGO_TARGET_DIR=/home/bobby/.cache/suss-m4-work/m4-cost-diagnostic-target,
CARGO_BUILD_JOBS=1 and RAYON_NUM_THREADS=1, without setting RUSTFLAGS:

```sh
cargo test --locked -p suss-cli --test compiled_macro_source_tags --test compiled_macro_collection_source_asts --test compiled_macro_core_environment -- --test-threads=1
```

Terminal exit0, **6passed/0failed/0ignored**:

| Target | Passed | Harness seconds |
| --- | ---: | ---: |
| compiled_macro_collection_source_asts | 3 | 360.27 |
| compiled_macro_core_environment | 1 | 470.29 |
| compiled_macro_source_tags | 2 | 128.74 |

Both caller phases and original assertions execute. The collection corpus,
textual-order/effects and inference checks now complete. The empty-do and
invocation-tag default-budget controls remain passing. These wall times are
observations, not comparative benchmarks. Raw log/terminal receipt:
`/home/bobby/.cache/suss-m4-work/pr252-test-budget-calibration/`.
The later AOT source changes were applied after these executables had been built;
this six-test result is attributed to6d, not the later source.

## AOT configuration and diagnostic regression

The follow-on exposes optional `suss compile --compile-fuel N` for all four
native compile modes. Omission retains defaults; zero is explicit. The override
configures the compilation Macro Session, never generated Runtime execution.
Existing default library wrappers remain. See the user-facing AOT preparation
document for API scope and limitations.

A direct pre-change CLI probe exhausted original default fuel during compiled
macro invocation at bytes97..110, preserving its prior output, but omitted
`app.sus`. Exit1,16.0055s. Binary SHA256: `56067902f7ef219d29ec39471084ff1ac72edda199556e891646f1e21e007ffa`.
Raw fixture/argv, stderr and terminal receipt:
`/home/bobby/.cache/suss-m4-work/pr252-aot-filename-before/`.

The repair adds selected-input filename context to AOT Compile diagnostics,
preserving their variant, span, stage, non-Compile errors and pathless behavior.
This is outer input context; it does not invent dependency-specific span ownership.

On the reviewed follow-on source with normally regenerated bootstrap:

- Bootstrap generation and identity verification: exit0.
- CLI parser defaults/all modes/unsigned values:2passed/0failed.
- Actual Macro options None/zero/10B:1passed/0failed.
- `compile_file_fuel_exhaustion_preserves_output_and_recovers_without_budget_changes`:
  1passed/0failed/0ignored,105.06s. Both original-default failing compiles require
  filename, exact byte span and invocation-stage fuel diagnostics; one preserves
  sentinel output and one creates no output. Subsequent compile with identical
  flags/defaults validates/instantiates the actual component and returns42 under
  this module's unchanged100M execution allowance.

Commands, stage logs, source patch and bootstrap hashes:
`/home/bobby/.cache/suss-m4-work/pr252-aot-compile-fuel-validation/`.
Independent source reviews cover option propagation, filename context and all
new regression sources. At this publication, file/namespace/main/project explicit
command controls, async controls, public-record execution, lazy-result exhaustion
and the adjusted macro-interrupt test have no completed result in this run.
No full workspace or final-head CI result is inferred.

## Source bindings for the executed follow-on

Production files (the accompanying commit supplies their exact contents):

- `crates/suss-cli/src/args.rs`: `abf0fc6f715fa88e5dd017120f4804cf125e99e34aab52386a44fac63a484575`
- `crates/suss-cli/src/main.rs`: `fc101b55435a71da3e60eb82f329b3e642daeea53b971c7ee0a5aaa5c3fe728a`
- `crates/suss-compile/src/portable_aot.rs`: `650a53946c836279fe3cf2e51ccc92f2836828c2a845fed72612fee90167d0ac`
- `crates/suss-compile/src/portable_project.rs`: `966270750fb3f21bdf906df2b1f19b96ab9d45189ef2dc0c35c1e6a840c2cf04`

Bootstrap payload hashes:

- `runtime/bootstrap/macro.json`: `75eb0b1bafb9c9c7ade58980ab617c90276031e203b6db3673add4d99b2fd4f9`
- `runtime/bootstrap/macro.wasm`: `4e015e2a346541cbcfc800b3222dc1f872a0c9150848711174330a89cbadcb3d`
- `runtime/bootstrap/runtime.json`: `071be5deb9dfa603d5572a68fddc2ecbd3cb4458bc9118337112da86dfadd9aa`
- `runtime/bootstrap/runtime.wasm`: `195db91e00c8d6b1efe7a71370c0cd4a272dbafbb6760017276c570cfe569cc7`


## Completed follow-on execution

The same one-shot source3e7 pipeline has now finished, terminal exit0:
**23passed/0failed/0ignored** across ten Rust test selections, plus bootstrap
and identity success. Production/bootstrap bytes also match the laterf4 commit;
f4's new large-finite test was not selected in this run. This23 includes the
four earlier Rust positives above; the cohorts are not additive.

| Selection | Passed | Harness seconds |
| --- | ---: | ---: |
| CLI parser options | 2 | 0.00 |
| Actual Macro options | 1 | 28.30 |
| Original-default AOT publication/recovery | 1 | 105.06 |
| File/namespace compiler-only command configuration | 1 | 202.94 |
| Main compiler-only command configuration | 1 | 98.96 |
| Project compiler-only command configuration | 1 | 100.60 |
| Existing async-import file controls | 7 | 53.71 |
| Calibrated public-record target | 7 | 284.70 |
| Infinite lazy expanded-result exhaustion/recovery | 1 | 46.96 |
| Macro interruption/span/recovery | 1 | 47.26 |

Public-record execution includes all nine real CLI export invocations, three
in-process semantic tests in both phases and three adversarial decoder guards.
The lazy-result test requires `expanded result transport`, exact call span,
ordered Macro effects12, absent pending Runtime binding, retained old value7,
unchanged caller stats and subsequent42 at the same2M compiler setting. The IRQ
test retains its originalu64MAX configuration and actual same-setting recovery.
This does not establish cancellation specifically during lazy realization.
Raw stage logs/counts/terminal receipt remain in the cache path above. Full
workspace/final-head CI are separate validation, not inferred from23 positives.

## Obsolete builder-guard assertion

On unchangedf4 the exact selector
`compiled_macro_data_builders::native_macro_form_transport_is_bounded_data_and_adds_no_resident_code`
failed at the depth64 quote: `unwrap_err()` received `Ok(SessionValue)`.
Terminal101,0passed/1failed/0ignored,41.54s. Later vector/string expectations and
the second caller phase were unexecuted in that failed run. Raw log and source
hash receipt: `/home/bobby/.cache/suss-m4-work/pr252-stale-builder-before.log`
and `pr252-stale-builder-before.json`.

Only its three obsolete structural-rejection expectations are replaced by actual
same-size finite quote/GC/full readback:64 list levels with nil leaf,4096 nil
vector elements and1048577 UTF16 units65. Original metadata, effects, source spans,
Session budgets, resident-code counters and external-handle equality remain.
The corrected selector has now passed in the ongoing full builder target; the
remaining target cases and separate compiled large-result target are not yet
terminal. No additional production or allowance change accompanies this repair.


## Complete builder and large/deep pipeline results

The full corrected builder target is now terminal:7passed/0failed/0ignored,
226.15s, with original Session budgets and all old resource/handle checks. In the
same Cargo command, the new large-result fixture's first Runtime wide-argument
case completed compilation but returned an evaluation `SessionError::Trap` for
fuel exhaustion at its initial10M caller allowance. The new target was0pass/1fail,
47.82s; later cases/readback were unexecuted. The overall command exited101.
This is distinct from a Macro expansion-stage Compile diagnostic and is not a
4096-node rejection. Raw log: `pr252-builder-and-large-finite.log`; original new
fixture and terminal classification: `pr252-large-finite-first-result/`.

The new positive fixture now specifies100M evaluation, independently from its10B
compilation allowance. No existing Runtime regression/default or original test
program/value/metadata assertion is changed. A single fresh execution of this
configured fixture is terminal0:1passed/0failed/0ignored,75.73s. All four cases
(4097-wide/80-deep inputs and tiny-input transformer-generated wide/deep results)
execute in both Runtime/Macro caller phases. Every element, nesting level, root
metadata17 and leaf metadata91 survives full readback after GC. No dimensions,
metadata or assertions are reduced. The explicit caller setting remains fixed
throughout each phase; the test itself performs no retry or budget escalation.

Commands use the same shared target/jobs1/Rayon1 environment without RUSTFLAGS:

```sh
cargo test --locked -p suss-cli --test compiled_macro_large_finite -- --test-threads=1 --nocapture
```

Raw log/receipt: `pr252-large-finite-explicit-evaluation.log` and
`pr252-large-finite-explicit-evaluation-result.json` under the existing cache root.
These results use the unchanged3e7 production/bootstrap bytes and the shown
CLI-test configuration; the later declaration-provenance metering repair is not
attributed these pre-repair executions. The remaining four preservation targets
and required full baseline/CI are separate execution scopes.


## Completed remaining preservation cohort

Producer42764 is terminal0 and drained:15passed/0failed/0ignored over four full
targets: compiler-catalog5(929.68s), constructor2(902.58s), control-source5(212.00s)
and declaration-metadata3(234.21s). Original programs/assertions and caller limits
remain. These executables were built before the subsequent provenance edit;
results bind to pre-repair3e7 production/bootstrap, not the new metering code.
Raw log/terminal receipt: `pr252-remaining-preservation.log` and
`pr252-remaining-preservation-terminal.json` under the existing cache root.

## Metered declaration provenance and deterministic lazy interruption

The original declaration-provenance formatter/hash allocated and processed the
full Form/source/path after publishing the replacement, without explicit billing.
It now streams the identical legacy bytes through SHA256 with conservative host
charges and cancellation polls before hash updates/finalization. All fallible
provenance work completes before replacement evaluation/publication. Fuel/host
errors retain their variants/downcasts and gain declaration span/path context;
interrupt display/classification retain their existing behavior. Successful
source-dependency hashes remain unchanged.

Normal bootstrap regeneration and identity pass. The first lib-test compile
stopped with E0433 in the new IRQ child's trait path; none of the four tests ran.
Only that path was qualified with `crate::portable`; bootstrap was regenerated
for the changed compiler-source identity. Raw failed run remains in
`pr252-provenance-and-lazy-irq-validation/`.

Fresh V2 execution passes the exact selected3provenance tests(47.18s), covering
rich/deep/UTF16/native-path legacy digest equality; real typed fuel and IRQ failure
with same-policy recovery; and a replacement failure that retains old hash,
implicit-input/catalog policies and actual old17 expansion at2M. Source snapshot
setup bills4,203,965units under the existing10M setup operation before that measured
2M operation; no free parsing or cold2.1MiB-under2M claim is made.

The exact deterministic lazy-result IRQ selector also passes1test(73.11s), both
caller phases. A fresh correctly typed test-only native closure signals entry
AFTER the lazy thunk's ordered effect12, waits for an interrupt acknowledgement,
and calls the original guest spin; no interrupt error is fabricated. Channel
synchronization selects the stage, while bounded receive timeouts only guard
harness liveness. All paths join the thread. Assertions require exact call span,
expanded-result-transport/Interrupted diagnostic, one callback, unchanged caller
stats, absent pending binding, old7, effect12 and GC/42recovery at the same2M.
The original macro/source APIs gain no production hook or widened visibility.

Commands use the same shared target/jobs1/Rayon1 environment, without RUSTFLAGS:

```sh
cargo run --profile test -p suss-cli --bin suss-bootstrap --locked -- runtime/bootstrap
python3 scripts/verify_bootstrap_identity.py
cargo test --locked -p suss-compile --lib portable_macros::declaration_provenance_tests:: -- --test-threads=1 --nocapture
cargo test --locked -p suss-compile --lib portable_macros::lazy_result_interrupt_tests::lazy_result_interrupt_is_synchronized_after_thunk_effects_and_recovers -- --exact --test-threads=1 --nocapture
```

Source patch, new child payload, bootstrap hashes, count-guarded receipts and raw
logs: `pr252-provenance-and-lazy-irq-validation-v2/`. Independent reviews are in
`pr252-metered-declaration-provenance-source-v3/` and
`pr252-lazy-result-interrupt-provenance-v3-integration-independent-review.json`.
Later unchanged stale-cap audit selectors are not included in these four passes.

## Additional CI scope

Head4baf run39187192555 has actual foundation success and completed integration
job failures. Six newly reached correctness cases exhaust Macro fuel (five &env,
one invocation); four CLI-test files now set only these six Macro allowances to
10B. Other bytes, Runtime limits and explicit negative/default controls are
retained. Replays are unexecuted at this publication. Six completed job logs and
head/job metadata are retained under `pr252-4baf-ci/`; the overall run was still
active when downloaded, so no complete-suite or unexecuted-scope count is inferred.

CI also confirms stale finite display/map assertions, including a valid2049-pair
nil-filled map with4098 backing slots. Original selectors are being executed
unchanged before any test repair. A separate actual pre-repair CLI case with nine
mixed constants returns the old eight-constant compiler error (exit1,22.142s),
where the accepted expression should select default19. Its binary hash/argv/raw
stderr are in `pr252-mixed-case-before/`. That production guard is still present
at this publication; prepared repairs have no executed or completion credit.


## Finite-limit audit and native control recursion

The unchanged quota audit preserved four obsolete rejection failures and an actual
native stack overflow in the original257-operand `and` case. The latter is a
compiler defect, not an obsolete assertion. Its repaired full-target replay also
aborted; tests after that abort remain unexecuted. The completed targets in
`pr252-finite-cap-removal-validation-v1` are maps6, native entrypoints10, case8
and Murmur5, all passing; control flow completed one case before SIGABRT.

Core process2252453 identifies repeated `Analyzer::control_form` frames bypassing
the outer growable-stack boundary. The diagnostic receipt and backtrace are in
`/home/bobby/.cache/suss-m4-work/pr252-control-stack-crash/`; the extracted temporary
core was removed. The repair guards direct `and`/`or` and `cond` tail recursion
with the existing analyzer stack-growth pattern. Charges, evaluation order,257
operands and default budgets remain unchanged. Normal bootstrap generation and
identity pass for this repair. Execution is ongoing in the single producer:

```sh
cargo test --locked -p suss-cli --test portable_control_flow -- --test-threads=2
```

Subsequent stages in
`/home/bobby/.cache/suss-m4-work/pr252-control-stack-fix-validation-v1/`
run the six calibrated Macro correctness cases across four complete targets and
the full declaration-functions target. They have no results yet. Full baseline,
independent final review and passing final-head CI remain required.


The stack repair's complete control-flow target is terminal3PASS/0FAIL/0ignored
(22.51s including build,2.83s test execution), including original257-and, ordered
257-and, all81 pinned observations and malformed/publication controls. Added
both-phase257-operand `or` and257-pair `cond` late/early selection regressions
have independent source review but remain unexecuted at publication. They check
ordered effects, short circuiting, selected-arm count and readback after GC at
unchanged default budgets. Compiler payload identity passes again. The ongoing
calibration stage has completed the field-reference target1PASS75.95s; subsequent
targets and the seventh calibrated declaration case remain pending. These are
focused results, not final-head workspace/CI acceptance.


Failed-shard CI refresh for8ae is terminal cancelled overall. Three completed
failed shards contain80PASS/4FAIL across21 harnesses, not all-run totals. Three
failures are the stale display/map expectations repaired in0b05b4c. The fourth,
`completed_declaration_hint_boundaries_match_actual_primary_expression_tags`,
exhausts compiler fuel in caller &env transport23..62. It now uses the same shared
10B Macro allowance; its Runtime budgets,23 original observations and assertions
remain byte-identical. Independent tiny-diff review passes; calibrated execution
is pending before the baseline. Failed logs/metadata remain in
`/home/bobby/.cache/suss-m4-work/pr252-8ae-ci-failed-jobs-refresh/`.


## Additional calibrated semantic execution

The existing producer44278 has completed field-reference1PASS75.95s,
quote-AST1PASS74.63s and record-projection2PASS779.28s, each with original
observations and Runtime limits. These executables bind to the recorded
pre-scanner source/bootstrap snapshot under
`/home/bobby/.cache/suss-m4-work/pr252-control-stack-fix-validation-v1/`.
The full source-macro target and subsequent declaration-functions target remain
running/pending; no result is inferred. A dependent controller queues complete
control-flow5, source-hint1 and the new repeated-helper companion1 only after
this run succeeds. The companion checks real guest increments, explicitly reads
zero after warmup reset, conservatively subtracts one potentially incomplete
transformer, requires more than one completed transformer, and checks retained
effects/no publication/same100k-budget recovery. It is unexecuted at this capture;
the original512-call negative remains unchanged.

A second dependent controller applies the independently reviewed native
parameter-observation metering patch only after those checks succeed, regenerates
BOOT normally, verifies identity, and executes its three focused regressions and
the complete cumulative-fuel target. Both dependent controllers are waiting on
verified live process identities; they start no parallel Cargo producer. Failed
prerequisites stop the chain. Full baseline and final-head CI remain unexecuted
acceptance gates, not implied by these selected passes.


## Metered definition observation execution

The preceding controller89334 completed0/drained: complete control-flow5PASS
9.72s, hint-boundaries1PASS90.93s (all23 observations in both phases) and the
completed-helper companion1PASS49.87s at unchanged100k Macro allowance. The
companion proves a conservative lower bound greater than one completed transformer,
with explicit zero reset readback, no pending definition, retained effects and
same-budget recovery. Original512-call exhaustion regression is unchanged.

The reviewed parameter-observation repair is now applied. It prepays visits,
reference allocation/relocation/fills and equal-length symbol comparisons to the
active Store; both scans retain original order and run before replacement
publication. Normal bootstrap generation and identity pass. Focused command:

```sh
cargo test --locked -p suss-compile --lib parameter_observation_ -- --test-threads=2 --nocapture
```

Actual3PASS/0FAIL/0ignored,63.27s including compilation/45.99s execution. Real
1000-unit scope decreases1000->919->838 before repeated scans exhaust it; typed
IRQ and same-budget retry pass. The public API test reaches an actual &form scan
OutOfFuel at40056 units,bytes0..998, preserves old17 definition/provenance/catalog
and implicit-input policies, then expands the old definition at that same budget.
Raw source patch, BOOT hashes, command receipts and logs are in
`/home/bobby/.cache/suss-m4-work/pr252-metered-parameter-observation-validation-v1/`.

The same sole producer now executes the entire12-test cumulative-fuel target:

```sh
cargo test --locked -p suss-cli --test compiled_macro_cumulative_fuel -- --test-threads=2
```

No terminal result for that target is claimed yet. The next gate after focused
success is Java/Node-free bootstrap verification followed by the exact required
workspace command. Final-head CI/review and all remaining issue criteria remain
separate. Earlier10 calibrated-target passes and declaration2 remain attributed
to their recorded pre-scan source/bootstrap; no new-source replay is inferred.
