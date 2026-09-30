# M2-03 runtime ABI acceptance

Issue #10 is the stable runtime foundation work package. Its objective is the GC
prelude, binary64 numbers, UTF-16 strings, closure ABI and binding cells. All four
published acceptance criteria are fulfilled on the reviewed implementation stack
through PR #63 (`791297524ab1aaa2bafc27d28c596c00ab6de771`). The acceptance PR
uses `Closes #10`; GitHub closure follows merge into the default branch. The stack
is still unmerged. This is not a claim that main or the M2 milestone is complete.

Because this PR initially targets its predecessor branch, its description's
closing keyword becomes effective after retargeting to main when that predecessor
lands. The review commit also contains `Closes #10`, so closure follows that commit
reaching main through the stack. Preserve the keyword in any squash message.
See [GitHub's closing-keyword rules](https://docs.github.com/en/issues/tracking-your-work-with-issues/using-issues/linking-a-pull-request-to-an-issue).

| Published criterion | Executing evidence |
| --- | --- |
| Cross-fragment values survive GC | `runtime_abi_generated_fragments_share_scalar_types_and_gc_roots` constructs independent runtime/producer/consumer modules, forces GC, then reads exact negative-zero bits and a lone surrogate. `runtime_abi_closures_check_arity_and_keep_old_captures_after_rebinding` calls original/current closures across modules after binding replacement and GC. Native `persistent_session_old_closures_and_owned_values_survive_rebinding_and_gc` covers compiled source inputs, captured values and retained UTF-16 handles in one Store. |
| Generic invocation checks arity | Shared `invoke` checks callable and argument-array types and closure minimum/maximum before entering its Invoke reference. The cross-fragment closure test verifies lower and upper fixed bounds and variadic maximum -1, catching the language exception and inspecting its UTF-16 message. Source closure tests cover known arity diagnostics and dynamic language failures. The arithmetic closure test executes empty/unary/variadic bodies and descriptor-identified wrong-arity failures for zero-argument - and /. |
| Float/UTF-16 corpus passes | Runtime tests independently inspect f64 fields for rounding, signed zero, infinity and NaN payload storage; UTF-16 tests inspect astral pairs and lone units after GC and reject truncating writes. All 194 source cases match fresh pinned ClojureScript observations and execute independently decoded generated fragments, including numeric coercion, UTF-16, dynamic values and closure recurrence. The separate 1,024-sample numeric matrix checks formatting/parsing against the pinned oracle and actual runtime. |
| ABI mismatch fails before execution | Manifest tests reject compiler/runtime/wasm-tools version changes, missing/duplicate/malformed manifests and changed actual recursive prelude even with a matching manifest. A host-marking initializer proves version/layout rejection precedes side effects. Native Session verifies and validates every artifact before staging cells, then links/instantiates all fragments before publishing declarations or evaluating source initializers; its private regression preserves counters/state on malformed-artifact and missing-cell-import failures. The changed-layout pre-effect regression is the separate runtime host-marker test. |

Tests are in [runtime_abi.rs](../../crates/suss-compile/tests/runtime_abi.rs),
[portable_closures.rs](../../crates/suss-compile/tests/portable_closures.rs),
[portable_pipeline.rs](../../crates/suss-compile/tests/portable_pipeline.rs),
[persistent_session.rs](../../crates/suss-cli/tests/persistent_session.rs) and
[portable_session.rs](../../crates/suss-cli/src/portable_session.rs).

The original ten-type recursive ABI covers Number, UTF-16, argument array, Invoke,
closure, binding cell, descriptor, user object, exception and dynamic frame.
Rooting uses Wasmtime Store/owned handles; current globals read live cells while
captured functions retain their behavior after replacement. Every compiled
fragment uses the same prelude and selected required imports. No opaque integer
representation or unchecked nominal structural test substitutes for this contract.

Root and dispatched independent PR #63 review passed the required full workspace
baseline (including CLI rustdoc), compiler72, native Session24 and Python54; fresh
194 source observations match. Logs: /private/tmp/suss-portable-recur-full.log,
/private/tmp/suss-pr63-review-full.log, -compiler.log, -session.log, -python.log and
-oracle-comparison.log. Native graphs were sequential with CARGO_BUILD_JOBS=2 and
shared CARGO_TARGET_DIR; no RUSTFLAGS override. Final acceptance-PR independent
review and exact final-head CI are required before merge readiness.

The old prototype's separate differential baseline remains 9 passes/7 exact
failures/0 skips. It is not the replacement ABI corpus and is not called certified.
Legacy/manual/doc ignores are unchanged. Complete nominal protocol/exception/
dynamic-binding machinery belongs to #11; collection/string/core coverage, named/
multiple/variadic source signatures, compiled macros, production command/REPL
migration, canonical memory and target adapters remain in their own work packages.
M2 and issue #9 remain incomplete. Completing this foundation does not certify all
of accepted design sections 4–6 or close a future milestone. No upstream source,
runtime code, dependency, license or ABI layout is changed by this audit.
