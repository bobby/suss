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
