# M3 acceptance audit in progress

This audit preserves the published requirements of issues #12–#15 and accepted
design sections 6–7/9. M3 is not complete. Passing foundation tests do not prove
unimplemented macro, cancellation or accounting behavior. Each PR requires an
independent subagent review, significant fixes and successful exact final-head CI;
PRs are opened for user review and are not merged by the agent.

| Requirement | Current evidence | Remaining work |
| --- | --- | --- |
| #12: atoms and closures persist | `persistent_repl.rs` executes the actual native command; `portable_atoms.rs` retains real atoms/old contents across fragments and forced GC | Merged #119 has independent review and exact-head CI; complete artifact/core acceptance remains distinct |
| #12: initializers do not replay | Command effect counter/defonce and `persistent_session_initializers_execute_once_and_defonce_skips_effects` | Preserve this behavior through namespace reload and macro integration |
| #12: old values remain usable | Owned native roots, old callable contents, captured old atom owners and functions survive rebinding/GC | Preserve through remaining M3 integration |
| #12: fragments import stable ABI | Production Session validates and links ABI2 artifacts before initializers; compiler/runtime ABI and negative import/layout tests | Reconfirm final integrated acceptance head |
| #13: globals see redefinitions; captured function values stay stable | Native command and session live-cell/capture regressions execute actual fragments | Merged #120 executes four namespace_session acceptance tests; macro phase/cache/private integration remains |
| #13: compile failure/failed initializer preserve bindings | Command and session regressions distinguish compilation from executing language errors, preserve old binding and preceding effects | Verify actual reloaded namespace failure/retry and phase integration |
| #13: deterministic namespace errors | Immutable graph tests execute require order/reuse and assert located ambiguity/mismatch/missing/cycle errors | Merged #120 proves native load/reload and deterministic errors; macro import/cache/private policy remains |
| #14: isolated compiled macro session | Native Session now executes artifacts in separate Runtime/Macro Stores with phase-qualified cells; three compiled_phase_session tests execute core/state/root/dependency/reload/reset behavior. Explicit source defmacro registration now compiles bodies in Macro Store and expands runtime/dependency forms through HIR; three compiled_source_macros tests execute nested/variadic/shadowing/&form/redefinition/error behavior. Six compiled_repl_macros regressions now execute native prompt definitions/calls, load/reload, source aliases/conditionals and two-phase reset failure. Explicit source macro imports now execute through the isolated Store with source-order dependency initialization and separate aliases/refers; legacy `expand.rs` still invokes `MacroEvaluator` | Integrate source defmacro expansion with the isolated compiled session and actual macro language data; remove evaluator after bootstrap acceptance |
| #14: syntax quote/unquote/splicing, deterministic gensyms, &form/&env | Bounded builtin expansion is prerequisite evidence only | Executing compiled macro tests for all named features and source spans/environment |
| #14: phase dependencies | Compiler module plans retain phase-qualified identities | Explicit source phase loading/isolation/failure tests execute; explicit macro libspec reload/reload-all executes source refresh and helper isolation; complete ordinary reload/privacy/inference/cache policy remains |
| #14: reproducible bootstrap without Java | Imported core has source/patch/license hashes; this is not compiled macro bootstrap | Versioned bounded bootstrap artifact and `scripts/verify-bootstrap.sh` without installed Java |
| #14: cache invalidation on macro changes | No complete compiled macro cache exists | Keys include source/compiler/ABI/macro graph/target/flags; changed-macro and unchanged-cache execution tests |
| #15: exceptions return to prompt | Actual command proceeds after compile/initializer/language errors; native host also recovers after fuel traps | Preserve through scheduler/async implementation |
| #15: cancellation cleans up, including pending interactive I/O | Readline Ctrl-C clears buffered input only; fuel exhaustion is a trap, not cooperative cancellation | Running and pending-I/O cancellation, rooted continuations, try/finally/dynamic scope, at-most-once resume/race tests |
| #15: reset releases session state | Replacement Store discards bindings/module identities/instances, rejects stale handles; failed atom core reprovisioning preserves old session | Integrated runtime/scheduler/resource cleanup proof |
| #15: code/heap counters distinguish retained code from leaked values | Resident fragments/input artifact sizes and external handles are distinct; heap capacity explicitly is not live memory | Actual live GC memory/object accounting and repeat/reset leak evidence |

Proposed named acceptance commands remain required: `cargo test -p suss-cli
persistent_session`, `cargo test -p suss-cli namespace_session`,
`scripts/verify-bootstrap.sh`, and `cargo test -p suss-cli session_lifecycle`.
A missing/empty suite is not passing evidence. Run focused regressions first and
then `cargo test --workspace --locked -- --test-threads=2` on frozen final code.
Issue closure requires a final requirement-by-requirement audit, not this progress
matrix. Full portable atoms/collections and target release gates retain their
separate milestone scope; no future work is marked complete here.
