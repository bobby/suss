# M3 acceptance audit in progress

This audit preserves issues #12–#15, stable work packages M3-01–M3-04 and
accepted design sections 6–7/9. M3 is not certified complete. M2 issues #8–#11 are closed and their
merged prerequisite behavior is exercised by the integrated suites. Passing prerequisites or selected regressions do not replace
full requirement-by-requirement acceptance, independent per-PR review and successful
exact final-head CI. No acceptance criterion is removed or narrowed here.

## Current source and evidence, 2026-10-07

This audit inspects PR #221 on `portable/m3-source-futures`, including the
independently reviewed native observation repair at `ac1b4fd`. Main includes
merged #217, #218 and #220. The source is frozen while the required unfiltered
workspace baseline and exact-head CI run. Passing focused evidence below does
not substitute for their terminal results.

The earlier PR #182 and 2026-10-05 production audits recorded older snapshots.
Their test results remain historical evidence in [handoff](handoff.md), not
current descriptions of active entry points. Current compiler/CLI source has no
`MacroEvaluator` or `expand::expand_all`. The old evaluator component frontend
and byte-only expression route are retired, rather than newly supported targets.
GitHub issue status remains authoritative: #12 and #13 are open; #14 and #15 are closed. PRs #218 and #220 are merged into main `b0024c0`; implementation PR #221 is draft. Issue closure does not substitute for current acceptance evidence.
The full implementation objective remains unchanged.

| Requirement | Current evidence and remaining acceptance |
| --- | --- |
| One long-lived Store, shared runtime and incremental fragments; no source replay | Native `portable_session::Session` owns the Store, cells and installed instances. `persistent_session` and `persistent_repl` cover once-only initialization, later inputs and command recovery. Final integrated acceptance remains required. |
| Atoms, closures, nominal types and old values remain rooted and usable across fragments/GC | Existing atom process tests, `persistent_session_old_closures_and_owned_values_survive_rebinding_and_gc`, nominal identity/extension tests and owned-handle checks cover targeted cases. Inspect all required public session paths in the final audit. Full atom compatibility is separate M7 work. |
| Every fragment imports the stable recursive ABI; incompatible artifacts fail before initialization | Shared `runtime_abi`, artifact identity and native preflight checks have executing tests. Regenerated Runtime/Macro pairs and Java-free checks validate the compiler identity. The final audit must also prove no active production path uses an incompatible prototype backend. |
| Globals see redefinition, while captured function values retain old behavior | Native cell/rebinding, namespace and live-command tests exercise these separately. Source cache reuse must preserve the distinction. |
| Compile failure publishes no session bindings | Transactional input/module preparation and `persistent_session_compile_failure_is_atomic_and_language_failure_recovers` cover native cases. Macro graph replacement and failed declaration provenance have separate regressions. Audit all current public compiled entry points; the evaluator component host was removed by #215. |
| Failed definition initializer preserves its old binding; preceding effects are not rolled back | Persistent session, namespace retry and source definition regressions execute this distinction. Dependency initialization completed before the error is preserved. |
| `defonce` and initializers execute once, including bound nil/false | Named persistent session tests and dependency diamond/retry tests execute these cases; reload must not replay unrelated completed initializers. |
| Namespace/phase resolution, reload and errors are deterministic | `namespace_session`, macro import/reload tests and namespace/declaration oracle corpora cover aliases, source selection, ambiguity and selected policies. Published project dependency loading is explicitly rejected and belongs to later work. Final integrated execution remains pending. |
| Macros execute compiled Suss in a separate phase Store and namespace graph | `CompiledMacros`, `compiled_phase_session`, imports/reload and source command tests execute phase isolation and retained expansions. Runtime-only command exit access is denied in Macro tests. |
| `&form` retains reader data, explicit metadata and actual source provenance | `compiled_macro_form_source_metadata` and source-position tests compare selected pinned observations, including generated syntax with unknown locations. Keep all declared bounds/errors and verify the complete accepted contract. |
| `&env` exposes genuine portable source facts | Rooted `AnalysisGraph` and source environment/declaration/tag corpora cover selected records. Source tags, local/global references, compound forms, invocation children and constructor/type declaration facts have subsequently gained pinned and native executing evidence; see the source-AST entries in [handoff](handoff.md) and [compatibility inventory](../compatibility/README.md). Independent inspection found no additional missing original M3 environment requirement. The accepted portable contract does not require the complete upstream analyzer schema; unassessed compatibility remains explicit. |
| Syntax quote/unquote/splicing and deterministic gensyms | `compiled_macro_syntax_quote` and `compiled_bootstrap` execute selected forms, state across fragments, failed-input stability, lazy output and collection data. Complete contract acceptance still requires an explicit scope audit. |
| Bounded bootstrap expander and reproducible versioned artifacts without Java | Both phase pairs reproduce and execute through `scripts/verify-bootstrap.sh`; input mutation/corruption checks reject stale identity. Development JVM/Node oracles are not shipped dependencies. |
| Cache keys include source, compiler/runtime ABI, macro dependency graph, target and flags; macro changes invalidate appropriately | Artifact identity, `compiled_source_artifact_cache` and `compiled_module_cache` execute targeted graph reload, declaration recovery, cross-Store and reset behavior. The current fixed target/profile and supported dependency policy were independently audited; published dependencies remain later work. Final integrated execution remains pending. |
| Remove the tree-walking macro evaluator after compiled bootstrap succeeds | Removed by #215: current compiler/CLI source contains no `MacroEvaluator`, and the evaluator component host is retired. Merged main commit `85b6218` is the evaluator retirement evidence; this document does not infer current issue state from source history. Current integrated-head regression execution and bootstrap reproduction remain required after continuation changes. |
| Runtime exceptions return control to the prompt | Native REPL recovery and typed language exception tests execute this. Traps/host errors remain separately classified. Final acceptance must include errors across pending I/O/resumption. |
| Reset releases session state and invalidates old/foreign owned handles | Native reset/handle/phase/cache tests execute targeted cases. First-release generated code may stay resident until reset, as allowed by the design. Current reset requests task cancellation, invokes native pending-operation hooks at most once, drains bounded compiled cleanup and returns `ResetPending` with the old Store usable when cleanup remains blocked. The focused reset regression passes; canonical subtask/resource teardown and final integrated reset acceptance remain open. |
| Cancellation while interactive I/O is pending cleans up and resumes at most once | **Partial implementation, acceptance open:** actual compiled source continuations and queued cancellation now execute. Own cancellation bypasses catches, permits awaited finally to remain Pending, then settles Cancelled; a cancelled dependency is a distinct catchable failure. Native host completion roots and ownership guards execute through Session. Synchronous interruption remains a separate trap path. Actual Unix FIFO/PTY process tests and generated canonical component cancellation now execute pending I/O, awaited cleanup and resource release. Final integrated execution remains pending; non-Unix interactive async I/O is unverified. |
| Pending/Ready/Failed/Cancelled transitions preserve locals, handlers, dynamic scope and cleanup; handle races/reentry/fairness | `portable/ir/async.rs`, `portable/emit/async.rs` and `runtime_abi/async.rs` implement compiled stackless resume states, GC-rooted snapshots, resumable try/finally/dynamic regions, generation-checked registrations and FIFO turns. Executing source/scheduler suites cover pending captures, consecutive awaits, GC, failure/cleanup override, invalid await recovery, cancellation during pending finally and caller dynamic restoration. Verified source backedges yield with rooted state; focused tests cover completed-dependency progress, bounded queue/registration roots and yielding catch/finally regions. Ordinary synchronous callees remain atomic; Canonical callback ownership, generation, recovery and retirement regressions also execute; final integrated acceptance remains pending. No source interpreter or synchronous future flattening is used. |
| Code residency and live GC/leak accounting are distinguished | **Merged #217 evidence:** `4c3bb6d` adds exact post-collection Wasm GC live-byte measurements via a test-only logger for pinned Wasmtime 49.0.1's `allocated_bytes` record. `session_live_heap.rs` asserts exact baseline return for released handle-held, cyclic and cell-retained graphs in Runtime and Macro Stores, fixed resident-code counters, and fresh replacement-Store equality after repeated reset. This observes replacement state, not the old Store's drop. Current production `SessionStats` separately reports code/artifact sizes, handles, pending host requests, GC heap capacity and numeric memory capacity; it has no live-byte field. #217 did not add a production live-byte API. Focused suspended-task, dead-capture, stream graph and 10,000 varied task-call regressions measure actual post-GC bytes and fixed code/handle counts. Real pending-I/O tests separately verify FD release. These are not generic WIT/OS-resource 10,000-call stress; queue/root counts alone are not byte/leak measurements. |

## Current executing evidence and limits

The following terminal results were checked in the logs named by the handoff.
They cover different development snapshots, not one frozen final acceptance head.
All listed suites have zero failures/ignores; filtered focused runs are identified.

| Evidence | Checked terminal result | Scope |
| --- | --- | --- |
| `/private/tmp/suss-async-analysis-fixtures-fixed.log` | analysis 10/10; liveness 6/6, no filters | Lexical captures, await context including ordinary methods, executable regions and recurrence isolation. |
| `/private/tmp/suss-fairness-integrated-first.log` | source 12/12 (2.85s); scheduler 19/19 (4.36s), no filters | Actual generated runtime plus compiled source/resume execution, pending/failure/cancellation/GC and source CPU backedge progress. |
| `/private/tmp/suss-yield-regions-focused.log` | 1 passed, 12 filtered (0.25s) | Catch payload, cleanup outcome and dynamic frame retained across yields/GC; caller binding restored each turn. |
| `/private/tmp/suss-public-async-session-first.log` | public async API 4/4 (9.27s); Session 7/7 (11.19s), no filters | Compiled `suss.async` operations, first-terminal settlement, nil/nested payloads and native host completion. Predates host ownership repair. |
| `/private/tmp/suss-secondary-interrupt-first.log` | 1 passed, 72 filtered (0.18s) | Real secondary Interrupt during recovery preserves original OutOfFuel, restores caller/exception state and retires roots without replay. |
| `/private/tmp/suss-public-async-bootstrap-verify.log` | bootstrap 4/4 (13.74s), no filters; reproduction/identity verification succeeds | Fresh Runtime/Macro pairs reproduce with Java/Node absent. Later production changes require fresh regeneration/verification. |
| `/private/tmp/suss-host-owned-session.log` | Session 8/8 (6.61s), no filters, following fresh bootstrap generation | Host resolve/reject cannot bypass a task's pending cancellation cleanup; cleanup completes once. This is native transport, not canonical callbacks. |
| `/private/tmp/suss-reset-runtime-snapshot.log` | scheduler 25/25 (3.17s), no filters | Current runtime cancellation/reset prerequisites, including unique owner capture and root retirement. |
| `/private/tmp/suss-reset-pending-native-retry.log` | reset 1/1 (0.19s), no filters | ResetPending survives retries, native cancellation hook runs once, cleanup is completed before successful reset and late old handles reject. |

Subsequent execution supersedes the historical native frontend and canonical
absence claims. The generated production scalar component executes genuine
Pending canonical imports, callback resumption, cancellation with nested awaited
cleanup, and owned transfer-memory release. Its public source-file entry point is
`portable_aot::compile_file_with_options`. The supported mapping is one asynchronous
u32 import/export; arbitrary nested WIT future/stream transport remains later
interop work, and these tests do not certify that broader contract.

The Unix native frontend now uses a single Store-owner event loop, bounded
scheduler turns and an explicit `suss.io/read-byte` producer. The five process I/O
tests passed, including idle progress, real pending-FIFO cancellation/reset and
post-fuel-trap prompt recovery (`suss-native-repl-trap-diagnostic-final.log`). The
PTY test passed UTF-8 redraw/history, interruption of decoded and unread input,
awaited cleanup and terminal restoration (`suss-native-pty-persistence-final.log`).
The persistent REPL suite passed 8/8 after fixing comment-only input output
(`suss-native-repl-empty-input-retry.log`). Non-Unix interactive asynchronous I/O
has not been certified. A fresh final integrated run remains required.

Live suspended task graphs returned to actual post-GC byte baselines in three
rounds while resident code remained unchanged (`suss-async-live-heap-retry.log`).
The production `ScalarHost` wrapper latches quarantine before suspension and
clears it only after successful invocation completion; raw Wasmtime embedders are
responsible for equivalent lifetime handling. Stream runtime/service/source
wrappers execute 12/12 compiled-source and 9/9 raw-runtime regressions. Actual
post-GC stream graphs return to the warmed byte baseline across three rounds
without changing resident-code counters (`suss-stream-live-heap-first.log`).
These results do not replace the final acceptance gates.

The first named persistence gate stopped on an unsupported-array display
error; after restoring display-specific diagnostics, the sequential retry passed
29 persistence tests, 20 namespace tests and 15 lifecycle tests. The lifecycle
selector includes all eight native async I/O cases and the PTY case. Java/Node-free
bootstrap reproduction, checkout-independent identity checks and all four
bootstrap execution tests also passed. Logs are
`/private/tmp/suss-m3-main-{persistence,namespaces,lifecycle,bootstrap-verify}-retry.log`.
These validate the frozen display-repair snapshot; subsequent canonical repair
changes still require their focused tests and the final full baseline.
Independent full-diff review also found that canonical export cancellation event
6 traps before source cleanup. Repair and an executing component-to-component
regressions now pass for actual event6 delivery, pending awaited cleanup,
cleanup success/failure and real Unix FD release before acknowledgment. The
required workspace baseline is running; final-head CI remains pending.

## Current production entry points

| Production path | Current inspected state | Remaining acceptance |
| --- | --- | --- |
| Native REPL, expression and file execution | Compiled Runtime/Macro Session hosts; no tree-walking evaluator. | Final integrated persistence, namespaces, phase/cache/schema and lifecycle checks. |
| Public source/WIT, main, file/namespace/project compilation | Shared compiled source preparation and portable artifact assembly. | Preserve origins, dependency/phase initialization and typed diagnostics; execute supported targets at the final integrated head. |
| Public expression/cache methods | `compile_expr_cached` delegates to `compile_expr_with_info`, which calls `prepare_expression`; prepared bundles execute without replay through the compiled path. Legacy byte-only `compile_expr` is removed. | Artifact/value/cache regressions and final integrated review/CI, rather than prototype encoding-only evidence. |
| Former evaluator component CLI | Retired by #215; no production `evaluator::eval_to_string` route remains. | Future compiled component frontend support is not implied by retirement. Canonical pending-I/O acceptance remains unchanged. |
| Session lifecycle/accounting | Rooted source tasks, cooperative cancellation/yield, host completion guards, bounded reset drainage and exact test-side live-byte evidence are present. | Final integrated repetition of executed native/canonical lifecycle and suspended-task live-byte evidence; stream lifecycle acceptance, independent review and final-head CI. |

## Remaining acceptance work

Reexecute the existing genuine pending canonical async import regressions through
generated adapters and compiled continuations on the final head, with GC, once-only
callback consumption, reentry,
completion/cancellation races, caller restoration, subtask/waitable/result-buffer
release and late callback rejection after reset. Async exports must obey the
declared future-result contract; do not implicitly flatten a future returned as
a WIT value. WIT `result::err` remains an ordinary typed value, distinct from a
language exception, runtime trap or task failure. Design §9's streams,
bounded reads/writes, backpressure, explicit EOF, endpoint ownership and
cancellation remain accepted scope, not exclusions introduced by this audit.

Reconfirm exact live GC versus resident-code evidence with suspended/cancelled
continuations and host resources. Independent inspection of the portable macro
environment, namespace/dependency/cache and public entry points identified no
additional concrete missing requirement in the original M3 criteria. Reexecute supported source
`&form`/`&env`, phase dependencies, syntax quote/gensyms and cache changes to
executing evidence. The accepted design does not mandate the complete upstream
`cljs.analyzer` schema; unsupported source forms and unassessed compatibility
items remain explicit rather than fabricated successful records. Preserve source provenance and upstream licensing.

This inspection is a source/coverage audit, not final-head execution. The broad
constructor corpus uses an explicit 100M fuel budget; main also retains a focused
default-budget constructor regression. Published project dependencies remain
explicitly unsupported and belong to later work rather than an invented M3 gate.
The named commands and independently reviewed cancellation/10,000-call accounting
regressions have passed focused runs. Final frozen-head workspace acceptance,
bootstrap reproduction after the last source edit and successful final-head CI
are still required.

The named acceptance commands remain `cargo test -p suss-cli persistent_session`,
`cargo test -p suss-cli namespace_session`, `scripts/verify-bootstrap.sh` and
`cargo test -p suss-cli session_lifecycle`. Missing/empty suites are not passes.
Run focused regressions first, then the unfiltered
`cargo test --workspace --locked -- --test-threads=2` on frozen final code.
Each PR requires independent review, significant fixes pushed and exact final-head
CI; historical full baselines do not validate the uncommitted continuation head.
Use `Refs` for partial progress and `Closes` only for fulfilled issue criteria.
No PR may be merged without a later explicit user instruction. M3 acceptance
remains unproven pending the final requirement-by-requirement audit; this is an
evidence statement, not a claim that issue #15 is open.

## Final repair evidence awaiting integrated gates

At `ac1b4fd`, CLI binary unit selectors passed 12 native-host and seven event-loop
tests; process suites passed eight native async I/O, one PTY and eight persistent
REPL tests. Pure getter interruptions retain roots and wait for acknowledgment;
completed source submissions are observed once without replay. Interrupted reset
preflight preserves reset intent. Non-Interrupt post-submit tracking failures
quarantine the fresh submission and preserve its result before later source;
ordinary compilation/language errors still recover at the prompt. Ownerless or
uncertain mutation failures remain conservatively quarantined.

The first CI run exposed fixture assumptions and fuel sweep horizons after the
stream-aware runtime grew. Repairs retain the assertions: select the unique
mutable UID global, use the installed private cancellation entry in raw harnesses,
and extend sweeps to include both actual interruption and successful completion.
The native publication sweep observes actual terminal OutOfFuel boundaries at
4694–4697 for both resolve and reject. The canonical retirement fixture uses the
Session default 10M budget. The earlier failing CI and interrupted baseline runs
remain recorded in the handoff and are not counted as acceptance passes.

Both handwritten canonical caller fixtures (`.wat` and `.wasm`) are tracked and
required. Independent full-diff and repair reviews have no unresolved material
findings. The third full baseline is running on this frozen code; final-head CI
at `ac1b4fd` completed with foundation and integration partitions 0–5 and 7
passing, but partition 6 failed Linux PTY interruption. Promotion and M3
certification wait for the repaired head to pass all required gates.

## Closure boundary agreed with Bobby

Complete the original acceptance criteria of #12–#15 through PR #221: executing
incremental persistence, live bindings and namespace failure behavior, compiled
macro phase/form/environment/bootstrap/cache behavior, and interactive
exception/cancellation/reset/root accounting. Fix defects exposed by those
acceptance tests, obtain independent review and green exact final-head CI,
reconcile evidence, then promote for Bobby's review. Later interoperability and
compatibility work retains its separate scope. No merge is authorized.

The latest idle-dispatch repair adds a pure query covering scheduler readiness,
stream roots and native retirement obligations. It skips dispatch only when
these observations prove no service work is needed; it still observes terminal
reports. Once mutation-capable scheduler service starts, uncertain ownerless
interruptions remain quarantined. The withdrawal-journal regression explicitly
constructs an unpublished prepared phase-2 journal; it does not claim observation
of a published receipt or a particular fuel interruption window.
