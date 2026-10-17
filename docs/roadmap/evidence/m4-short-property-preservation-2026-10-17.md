# Short-property probe: preservation and matched parent — 2026-10-17

Tested component source: `59fdbad98c5da13d8098e3624863d6403e780365`.
This carries source4541f9d's probe repair plus four shared pinned CLJS/native cases.
[Earlier focused evidence](m4-short-literal-key-probe-2026-10-17.md) retains
113Rust passes/four separate CoreEnv/record/reduction fuel failures. Do not sum
those with this run as distinct coverage.

## Actual preservation execution

| Group | Passed | Failed | Ignored |
| --- | ---: | ---: | ---: |
| Shared native oracle, four cases/both phases after GC | 1 | 0 | 0 |
| Source/domain/raw-key guards | 46 | 0 | 0 |
| Core import | 17 | 0 | 0 |
| Pinned Jank map wrapper | 1 | 0 | 0 |
| Analysis graph | 11 | 0 | 0 |
| Namespace/declaration identity | 0 | 2 | 0 |
| Source tags | 1 | 1 | 0 |
| **Rust executions** | **77** | **3** | **0** |

Normal bootstrap exited0. Every planned stage executed. All2996 component
inputs stayed unchanged after bootstrap and resolve to published59fdbad/pinned
CLJS `c4295f303100bbf5afac449242d30bca1126f1a1`. SourceChild retains all220
ordered labels/results, five variants and both phases including physical/memo
negative tail. Final validation returns `Ok` with149643 fuel remaining in EACH
phase versus132163 in the published compact-head control:17480 more remaining.
This is the complete fixture's remaining counter, not a pure allocation/hash
subtotal or whole record/CoreEnv saving.

The unchanged map uses200M allowances in both sessions. Independent host
comparison retains69ordered IDs:62host passes/7guest judgments, zero failures,
skips or missing assertions. Guest judgments are not host-decided passes.

## Three failures, classified separately

- `compiler_namespace_and_declaration_aliases_match_pinned_five_call_observations`:
  both projections unexecuted after fuel exhaustion during compiled macro
  invocation184..224. This is NOT caller-&env transport classification.
- `resolved_callable_info_keeps_raw_declaration_children_across_macro_invocations`:
  all6rows return without request errors. First calls match in both phases;
  later4rows fail indices5/7/8: saved raw declaration and saved parameter-vector
  identity. Metadata aliases and fresh resolved outer-info behavior remain correct.
  This is an observed semantic failure, not unexecuted/fuel coverage.
- `compiled_source_inference_matches_pinned_tags_without_changing_executed_storage_or_effects`:
  caller-&env fuel382..414 before required32row/storage/effect assertions.
  The separate invocation-tag test passes.

A matched unchanged namespace pair on published parent
`f156df883187aee4281f703e853155fcd2e4ad51` executes0pass/2fail/0ignored.
All4026 tracked parent inputs are unchanged. Complete callable actual/expected
JSON is exactly identical across parent and candidate; the first probe has the
same both-phase fuel stage/span. These outcomes predate the probe. Exact whole
cost/progress of that fuel-blocked probe remains unknown; no whole-suite
regression-free conclusion follows.

Source inspection identifies the active reuse controller retaining metadata
only; staged declaration/field RootCache lookup/publication has no production
caller. This explains the alias pattern. A guarded coherent-controller fix is
proposed separately; this packet does not implement or certify it.

## Commands, artifacts and limits

The archived controller `run-short-property-observations-preservation.py` runs
normal bootstrap, then the seven selected groups with jobs1/Rayon1/test-threads1;
RUSTFLAGS inherited. Complete argv and selections are in raw receipts. The
shared four-path source packet manifest SHA256 is
`83031ff791090374c7d4eb93c35a8f36b445b1aa8256da94f97edcfe5351173f`,
patch `7eed3f3b6a89e98382371cd81a50b2e0060b4a2b55ba0e10c503dde336289aa1`.
The matched parent command is
`cargo test --locked -p suss-cli --test compiled_macro_namespace_identity -- --test-threads=1 --nocapture`.
No fixture, assertion, allowance or fee was changed.

[Raw archive](m4-short-property-preservation-2026-10-17.tar.gz): SHA256
`cc5726de23f4011b2a3377d250db9db39748fa11e13ae9587a7f570eb48cfb07`;45members/44hashed payloads. It retains all raw logs, map capture,
source packet, selections, five freeze maps, both explicit Git-binding sets,
source/execution independent reviews, six-row diagnosis and matched parent
comparison. Map/source-child raw evidence and all failures remain intact.
Candidate and parent freeze scopes differ (2996component vs4026all tracked);
both have exact public Git bindings. Git dependencies are required; not standalone.

No passing required full workspace baseline or final-head CI, issue closure,
promotion or merge is established. The four earlier CoreEnv/record/reduction
failures remain separate gaps. The unchanged namespace/inference failures remain
acceptance targets for#264/#261. The preserved probe/source/helper work may update
draft#268 while those targets remain unresolved.
