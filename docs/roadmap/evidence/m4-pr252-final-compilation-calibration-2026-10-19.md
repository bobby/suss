# Remaining public compilation allowances and finite comparison execution

Base source: `65e97187ff8a9492c9563797cbd291b683190e45`. Completed CI39207507722
executes all254integration targets:1716PASS/3FAIL/17existingignored. Foundation
passes444Rust/353Python. Every integration checkout tree equals the published
source and local committed tree. The three failures are explicit in retained
terminal logs, not hidden behind fail-fast selection:

- `portable_comparisons`: the original300-argument direct `<` test still expects
  a Compile error from the removed structural ceiling (6PASS/1FAIL).
- `public_compiled_files`: the original filename/lexical-environment/effects
  fixture exhausts compilation fuel during &env transport225..332 (12PASS/1FAIL).
- `uncached_expression_artifacts`: its original lexical macro/captured-bindings
  fixture exhausts &env transport153..248/internal39515..39524 (2PASS/1FAIL).

The exact local workspace producer was deliberately cancelled only after that
same-tree CI proof. Actual outer143, verified ownedPGID2717157 drained,102complete
passes retained; the interrupted compiler-catalog target has no terminal verdict.
It is not a successful full baseline. Earlier local temporary-storage EDQUOT and
its unchanged async2/TMPDIR recovery remain separately recorded in the handoff.

## Resulting behavior

`Compiler::compile_files_with_compile_fuel(source_path, wit_path, allowance)`
uses the existing typed `compile_input_with_options` route, preserving namespace
None, filename provenance, I/O/Parse/WIT/Unsupported distinctions and validation
order. Legacy `compile_files` delegates None. Exports/async-import defaults remain
unchanged; compilation fuel does not configure the returned component's Store.

`Compiler::compile_expr_with_compile_fuel(source, allowance)` uses the existing
uncached expression helper. Legacy `compile_expr_with_info` delegates None;
cached/prepared entry points and Macro namespace isolation remain unchanged.
Both new options distinguish explicit Some(0) from None. New controls exercise
I/O/WIT preflight before throwing macros, explicit-zero Semantic fuel failure,
same-Compiler recovery through legacy/None/10B and actual artifact execution/GC.

Only the two observed semantic fixtures now pass the existing10B compilation
allowance. Exact inverse checks recover both original test bodies, including all
source, Runtime settings, metadata/file/locals observations, captured/live binding
behavior, initializer effects and GC assertions. No shipped10M default changes,
Runtime allowance increases, helper replenishment, oracle edits or new ignores.

The comparison case keeps all300arguments, original Runtime budget and runtime
alias path. The direct form now executes and returns true; a second full300-arg
form with equal final operands returns false. This prevents a truncated prefix
from qualifying as success. The older compile-error expectation described the
removed resource ceiling, rather than portable comparison semantics. No compiler
comparison implementation changes are needed; all remaining comparisons/tests
and typed error/recovery controls remain intact.

## Actual validation and retained failure history

An initial author build fails E0308 in5.319s because fuel was accidentally passed
to the old helper's namespace argument. No tests execute on that candidate. V2
corrects the forwarding through existing AotOptions/namespaceNone; independent
source review approves the corrected route, typed error/default preservation,
original fixtures and new controls. The failed build and source patch remain.

With CARGO_BUILD_JOBS=1/RAYON_NUM_THREADS=1, the existing external target dir and
explicit work-cache TMPDIR:

```sh
cargo run --profile test -p suss-cli --bin suss-bootstrap --locked -- runtime/bootstrap
python3 scripts/verify_bootstrap_identity.py
cargo test --workspace --locked --no-fail-fast --test portable_comparisons --test public_compiled_files --test uncached_expression_artifacts -- --test-threads=2
```

Fresh normal bootstrap generation passes18.67s and identity verification1.01s.
Both phases' six non-custom Wasm sections are identical to65; source/custom
identity and hashes are regenerated. The workspace comparison target name selects
both the7-test CLI target and2-test compiler target; the complete selected cohort
is27tests, not25. No zero-test/filtered selection is substituted for execution.

The complete selected run exits0 in464.20s: CLI comparisons7PASS/0FAIL/0ignored
(7.78s), compiler comparisons2/0/0(25.77s), public files14/0/0(332.77s),
uncached expressions4/0/0(74.35s). Actual outer session55797 exits0 and is drained.
The27positive results include all three original failed cases and both new
explicit-zero/default-recovery regressions; all four harnesses have zero filtered tests.

The reviewed TMPDIR handoff appendix is included with these source repairs.
Existing source-bound #252 evidence and known unsupported core cases remain
qualified; no whole-core or milestone completion is claimed. Fresh Java/Node-free
reproduction, the exact unfiltered workspace command and final-head CI remain
required after publication. No closure, draft promotion or merge is claimed here.

Raw logs, source patches, terminal receipts and SHA-256 file manifest are in
[m4-pr252-final-compilation-calibration-2026-10-19.tar.gz](m4-pr252-final-compilation-calibration-2026-10-19.tar.gz).
Archive SHA-256: `d97c85f198f0fba2b2e22d71225ca6e28d65ea12ed9288cd5f39dac4944eccf6`.
