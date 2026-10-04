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
| #14: isolated compiled macro session | Native Session now executes artifacts in separate Runtime/Macro Stores with phase-qualified cells; three compiled_phase_session tests execute core/state/root/dependency/reload/reset behavior. Explicit source defmacro registration now compiles bodies in Macro Store and expands runtime/dependency forms through HIR; three compiled_source_macros tests execute nested/variadic/shadowing/&form/redefinition/error behavior. Six compiled_repl_macros regressions now execute native prompt definitions/calls, load/reload, source aliases/conditionals and two-phase reset failure. Explicit source macro imports now execute through the isolated Store with source-order dependency initialization and separate aliases/refers; legacy `expand.rs` still invokes `MacroEvaluator` | Preserve executing source defmacro and macro-data integration while completing versioned bootstrap acceptance; remove the temporary evaluator only after replacement bootstrap tests pass |
| #14: syntax quote/unquote/splicing, deterministic gensyms, &form/&env | Compiled transformers receive actual caller forms and rooted compiler environment graphs. Executing suites cover persistent vector/map/set/sequence metadata data, local/declaration/function/method records, selected source inference and reader positions. [Source inference](../runtime/compiled-macro-source-inference.md) retains 32 pinned facts plus 11 executing results; reviewed PR #153 passed clean local and final-head CI at `1874372`, 1,015/0/17 existing ignores. [Reader metadata](../runtime/compiled-macro-reader-metadata.md) has nine actual primary observations, eight exact native projections and a separate canonical file assertion; reviewed PR #154 focused reader32/native10 passes at `a4fa3e2`, with full1027/0/17 baseline and final-head CI37101260084 passed. Reviewed #155 executes eleven compiled syntax-quote/transport tests, eighteen shared pinned macro observations and fifty lazy/constructor observations; eight live-cell regressions cover both caller phases. Reader namespace qualification, scoped phase-separated gensyms, nested quotes, splicing and metadata have focused coverage. PR #155 final-head0f3c160 has independent review fixes, full1063/0/17 baseline and successful CI37114173950; it is ready for user review without merging | Complete remaining portable environment/schema behavior and bootstrap acceptance; passing selected records does not certify the full requirement |
| #14: phase dependencies | Compiler module plans retain phase-qualified identities | Explicit source phase loading/isolation/failure tests execute; explicit macro libspec reload/reload-all executes source refresh and helper isolation; complete ordinary reload/privacy/inference/cache policy remains |
| #14: reproducible bootstrap without Java | Working versioned runtime/macro images restore immutable compiler facts and execute in fresh Stores. `scripts/verify-bootstrap.sh` passes exact two-process generation and four executing regressions with Java/Node unavailable on PATH; affected66 compiler/83 native pass. Independent PR #156 review fixed embedded numeric runtime identity and strengthened Store/reset isolation; final-head8dbcc679 full baseline1067/0/17 passes | Exact-head CI37117694147 succeeded; PR #156 is ready for user review. Remaining macro-cache/schema/evaluator acceptance stays open |
| #14: cache invalidation on macro changes | Reviewed #157 adds bounded native Module reuse after expansion/emission, keyed by exact Wasm bytes and Engine identity. Final head f528159a passed independent review, full1072/0/17 baseline and CI37119275510; it is ready for user review. It does not skip macro execution or cache source analysis | Keys include source/compiler/ABI/macro graph/target/flags; changed-macro and unchanged-cache execution tests |
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

Reviewed #158 complete core-namespace graph construction now passes in both caller phases at5d15a0d, with duplicate reader callback/result and metadata-set ordering fixes. Full1079/0/17 baseline and exact-head CI37124594996 succeeded; PR ready without merging. Selected function declaration field projections are the next partial schema change and remain subject to independent review/full baseline/CI.


# Current requirement audit after the official command review

The objective is all of M3 in [the accepted design](../design/suss-0.3.1.md),
with stable work packages [M3-01–M3-04](issues.json) linked to issues
[#12](https://github.com/bobby/suss/issues/12),
[#13](https://github.com/bobby/suss/issues/13),
[#14](https://github.com/bobby/suss/issues/14) and
[#15](https://github.com/bobby/suss/issues/15). None is complete on the evidence
below. This audit records requirements and gaps, rather than treating a green
prerequisite stack as milestone acceptance. M2 remains a declared dependency.

The inspected source baseline is PR #182's independently reviewed
`383d2062b80973135d767cc68a3398147ae46692`. Its required full baseline terminated0 with1178 passed/0 failed/17 existing
ignores/0 filtered across136 groups. Root independently counted the complete log.
Exact-head CI is still running at this audit point; the new scalar AST increment
has focused execution only, not a full baseline at its own final head. Earlier reviewed heads have counted
full-baseline and CI evidence in [the handoff](handoff.md). A named test below
identifies targeted executing coverage; it does not certify all cases of the
requirement or substitute for acceptance at the final integrated head.

| Requirement | Current evidence and remaining acceptance |
| --- | --- |
| One long-lived Store, shared runtime and incremental fragments; no source replay | Native `portable_session::Session` owns the Store, cells and installed instances. `persistent_session` and `persistent_repl` cover once-only initialization, later inputs and command recovery. Final integrated acceptance remains required. |
| Atoms, closures, nominal types and old values remain rooted and usable across fragments/GC | Existing atom process tests, `persistent_session_old_closures_and_owned_values_survive_rebinding_and_gc`, nominal identity/extension tests and owned-handle checks cover targeted cases. Inspect all required public session paths in the final audit. Full atom compatibility is separate M7 work. |
| Every fragment imports the stable recursive ABI; incompatible artifacts fail before initialization | Shared `runtime_abi`, artifact identity and native preflight checks have executing tests. Regenerated Runtime/Macro pairs and Java-free checks validate the compiler identity. The final audit must also prove no active production path uses an incompatible prototype backend. |
| Globals see redefinition, while captured function values retain old behavior | Native cell/rebinding, namespace and live-command tests exercise these separately. Source cache reuse must preserve the distinction. |
| Compile failure publishes no session bindings | Transactional input/module preparation and `persistent_session_compile_failure_is_atomic_and_language_failure_recovers` cover native cases. Macro graph replacement and failed declaration provenance have separate regressions. Audit all entry points, including component-host paths. |
| Failed definition initializer preserves its old binding; preceding effects are not rolled back | Persistent session, namespace retry and source definition regressions execute this distinction. Dependency initialization completed before the error is preserved. |
| `defonce` and initializers execute once, including bound nil/false | Named persistent session tests and dependency diamond/retry tests execute these cases; reload must not replay unrelated completed initializers. |
| Namespace/phase resolution, reload and errors are deterministic | `namespace_session`, macro import/reload tests and namespace/declaration oracle corpora cover aliases, source selection, ambiguity and selected policies. Published dependency loading is still explicitly rejected; final namespace/cache/policy acceptance is open. |
| Macros execute compiled Suss in a separate phase Store and namespace graph | `CompiledMacros`, `compiled_phase_session`, imports/reload and source command tests execute phase isolation and retained expansions. Runtime-only command exit access is denied in Macro tests. |
| `&form` retains reader data, explicit metadata and actual source provenance | `compiled_macro_form_source_metadata` and source-position tests compare selected pinned observations, including generated syntax with unknown locations. Keep all declared bounds/errors and verify the complete accepted contract. |
| `&env` exposes genuine portable source facts | Rooted `AnalysisGraph` and source environment/declaration/tag corpora cover selected records. Portable initializer AST operation/value/children schema remains incomplete; source callable/method/declaration metadata and remaining inference rules require complete evidence. The new five-case scalar trace passes fresh pinned analysis, and both new native regressions pass after repair; this remains partial schema evidence. |
| Syntax quote/unquote/splicing and deterministic gensyms | `compiled_macro_syntax_quote` and `compiled_bootstrap` execute selected forms, state across fragments, failed-input stability, lazy output and collection data. Complete contract acceptance still requires an explicit scope audit. |
| Bounded bootstrap expander and reproducible versioned artifacts without Java | Both phase pairs reproduce and execute through `scripts/verify-bootstrap.sh`; input mutation/corruption checks reject stale identity. Development JVM/Node oracles are not shipped dependencies. |
| Cache keys include source, compiler/runtime ABI, macro dependency graph, target and flags; macro changes invalidate appropriately | Artifact identity, `compiled_source_artifact_cache` and `compiled_module_cache` execute targeted graph reload, declaration recovery, cross-Store and reset behavior. Complete published dependency/target cache policy remains open. |
| Remove the tree-walking macro evaluator after compiled bootstrap succeeds | **Incomplete:** `suss-compile/src/expand.rs` still owns `MacroEvaluator`; public prototype Compiler paths remain. `suss-cli/src/component.rs` still calls an evaluator WIT interface. Native command migration alone does not meet removal. |
| Runtime exceptions return control to the prompt | Native REPL recovery and typed language exception tests execute this. Traps/host errors remain separately classified. Final acceptance must include errors across pending I/O/resumption. |
| Reset releases session state and invalidates old/foreign owned handles | Native reset/handle/phase/cache tests execute targeted cases. First-release generated code may stay resident until reset, as allowed by the design; pending task/resource cleanup is not yet implemented. |
| Cancellation while interactive I/O is pending cleans up and resumes at most once | **Incomplete:** the native Session has no rooted source continuation/pending-I/O scheduler. Fuel exhaustion is a trap/recovery test, not cooperative cancellation. Official async command completion is synchronous source execution and does not prove suspension. |
| Pending/Ready/Failed/Cancelled transitions preserve locals, handlers, dynamic scope and cleanup; handle races/reentry/fairness | **Incomplete:** design §9 is a prerequisite for the pending-I/O lifecycle gate. Need actual source future/await lowering and scheduler/adapter regressions, including finally, dynamic bindings, cancellation races and fairness. Generic synchronous callback success does not prove these. |
| Code residency and live GC/leak accounting are distinguished | **Incomplete:** `SessionStats` reports fragment/artifact bytes, external handles, GC heap *capacity* and numeric scratch capacity. It explicitly does not measure live objects/leaks or actual native JIT memory. Stable canonical allocator page count is not this gate. |

The milestone's planned filter commands are `cargo test -p suss-cli
persistent_session`, `cargo test -p suss-cli namespace_session`,
`scripts/verify-bootstrap.sh` and `cargo test -p suss-cli session_lifecycle`.
Existing lifecycle tests live in several modules; a passing filter with no
pending-I/O tests cannot close #15. Focused commands come first; every PR still
requires independent review/fixes, the unfiltered
`cargo test --workspace --locked -- --test-threads=2` baseline and exact final-head
CI. Use `Refs` for partial progress. `Closes` is reserved for the full issue gate.
No PR may be merged without a later explicit user instruction.

Next finish genuine source AST transport with executed native/pinned evidence,
then audit and retire the remaining evaluator paths and implement rooted
continuations/pending-I/O cancellation plus live memory accounting. Keep this
matrix aligned with implementation, inventory and actual terminal evidence;
do not turn outstanding requirements into exclusions to make M3 complete.

The subsequent source-local reference increment adds actual `:local` operation,
selected binding fields and shared declaration/initializer identity, with six
fresh pinned observations and both caller-phase native execution. This narrows
the unfinished reference schema; global/field references and full portable AST
schema remain open. It does not change any original M3 acceptance requirement.
See [scope/evidence](../runtime/compiled-macro-local-reference-asts.md).

Resolved global-symbol ASTs now retain genuine operation/name/namespace/info from
captured declaration revisions, with eleven exact fresh pinned/executed projections
and native GC/effect evidence in both caller phases. This narrows the unfinished
reference schema; field/quote/compound/invocation/children and complete portable
source records remain open. The identity docstring mismatch was repaired in the
provenance-tracked source adaptation, with both phase images regenerated. Full
review/baseline/final CI remain required for this new increment. No original M3
gate is removed; see [scope/evidence](../runtime/compiled-macro-global-reference-asts.md).
