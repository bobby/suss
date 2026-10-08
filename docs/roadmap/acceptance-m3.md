# M3 acceptance evidence

This record covers the original work packages M3-01–M3-04, issues #12–#15,
and accepted design sections 6–7 and the lifecycle requirements of section 9.
Implementation is delivered through PR #221. The closure target is independently
reviewed implementation, complete workspace verification, green exact final-head
CI and promotion for Bobby's review. Merge remains a separate decision.

## Criterion-by-criterion evidence

| Work package and original criterion | Executing evidence |
| --- | --- |
| #12: atoms, closures and old values persist | `portable_atoms::persistent_session_atoms_and_old_contents_remain_rooted_across_fragments_and_gc`; `persistent_session::persistent_session_old_closures_and_owned_values_survive_rebinding_and_gc`; nominal type/descriptor and owned-handle suites. |
| #12: initializers do not replay | `persistent_session_initializers_execute_once_and_defonce_skips_effects`; `persistent_session_failed_module_retries_without_replaying_successful_dependencies`; dependency-diamond source preparation and persistent REPL tests. Nil and false count as initialized bindings. |
| #12: fragments import the stable ABI | `runtime_abi_generated_fragments_share_scalar_types_and_gc_roots`; `runtime_abi_manifest_gate_precedes_initializer_effects`; cross-fragment roots, nominal identity and compiler/runtime artifact identity checks. |
| #13: globals observe redefinition; captured functions stay stable | `namespace_session_command_loads_once_and_keeps_live_globals_and_old_captures`; persistent closure and cell/rebinding tests. |
| #13: compilation and failed initialization preserve old bindings | `persistent_session_compile_failure_is_atomic_and_language_failure_recovers`; `namespace_session_reload_reads_changed_source_and_preserves_bindings_on_failure`; macro declaration recovery and phase transaction tests. Already-performed effects are preserved, not rolled back. |
| #13: deterministic namespace errors | `namespace_session_errors_are_deterministic_and_reload_failure_keeps_dependencies_provided`; namespace acceptance, alias, ambiguity, reload ordering and source-origin tests. |
| #14: syntax quote and gensyms | `compiled_macro_syntax_quote_executes_unquote_and_sequence_splicing`; `compiled_macro_syntax_quote_reuses_auto_gensyms_and_evaluates_input_once`; syntax quote state/failure stability and bootstrap execution. |
| #14: genuine `&form` and `&env` | `compiled_source_macro_form_retains_reader_positions_on_nested_syntax`; `compiled_source_macro_environment_retains_lexical_initializer_and_shadow_without_reexecution`; compiled source AST/tag/position/declaration corpora with pinned observations. Unknown generated locations remain explicit. |
| #14: compiled isolated phase dependencies | `namespace_session_compiled_macro_imports_execute_phase_dependencies_and_aliases`; compiled phase session, macro imports/reload and runtime-only command denial tests. Current production paths contain no tree-walking `MacroEvaluator` or `expand::expand_all`. |
| #14: Java-free reproducible bootstrap and macro cache invalidation | `scripts/verify-bootstrap.sh` reproduces Runtime/Macro pairs twice with Java/Node absent, checks shipped artifacts and identity, and executes four bootstrap tests; `source_artifact_cache_uses_loaded_macro_graph_until_explicit_reload_all`, module-cache and reset/cross-Store tests. Keys incorporate the supported target/profile, flags, source/compiler/ABI and macro graph. |
| #15: exceptions return to prompt | Persistent REPL compilation/language recovery, native I/O catchable failures and recovered fuel-trap process tests. Language exceptions, runtime traps and typed host errors retain distinct handling. |
| #15: cancellation cleans up during pending interactive I/O | Real FIFO native I/O and PTY tests, including `session_lifecycle_cli_sigint_cancels_pending_streams_awaits_finally_and_retires_native_fds` and `session_lifecycle_interactive_utf8_redraw_history_and_pending_sigint_cleanup_are_preserved`; source finally/dynamic/fairness and canonical event-6 caller cancellation tests. |
| #15: reset releases session state | `session_lifecycle_cli_reset_waits_for_pending_stream_cleanup_and_late_bytes_cannot_cross_stores`; `cancellation_preflight_interrupt_preserves_intent_and_buffered_source` proves reset/exit intent and buffered input survive repeated getter interruptions; `cancellation_mutation_interrupt_clears_stale_observation_and_stops_input` proves uncertain mutation remains fatal; reset interruption/pending hook tests; two-phase/cache/handle invalidation and `repeated_reset_returns_live_bytes_and_code_to_a_fresh_session`. Cleanup blocked on input retains the old Store as `ResetPending`. |
| #15: resident code versus leaked values | Exact post-GC pinned Wasmtime live-byte tests for retained/released graphs, suspended tasks, streams and repeated reset; `dead_capture_graph_is_released_at_await_while_owner_remains_pending`; `ten_thousand_varied_tasks_retire_waiters_and_return_live_heap_to_baseline` uses independent Rust payload decoding and fixed code/handle counters. Production capacity counters are not mislabeled as live bytes. |

## Verification gates

The independently reviewed production implementation head is
`9e77ffcaaf2190c1bf6cf62df81b6829c2eab9d2`. Its required unfiltered local
workspace baseline completed successfully. The final head adds a reviewed test-only
SIGINT synchronization repair and this evidence reconciliation; production code is
unchanged. Promotion requires successful CI on that exact final head. Its terminal
outcome is recorded on PR #221 rather than requiring a self-referential commit.

- Full baseline: PASS, `CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=/private/tmp/suss-m3-source-futures/target cargo test --workspace --locked -- --test-threads=2`, exit 0. 201 terminal suite records: 1,531 passed, zero failed, 41 ignored, zero filtered. Toolchain and doc tests completed.
- Implementation-head [CI run 37740573919](https://github.com/bobby/suss/actions/runs/37740573919): terminal FAILURE; foundation and seven integration partitions passed. Partition 5 evaluated dependent input before SIGINT acknowledgment (`7500` before `^C`). The test-only repair waits for acknowledgment and a real pending cleanup FIFO reader, retains exact results and deadline, and adds once-only output and both FD-retirement checks. This transcript does not establish a production cleanup stall.
- Exact final-head CI: REQUIRED before promotion; foundation, all eight integration partitions and aggregate test must succeed. Consult PR #221 for the final-head result.
- Repaired real native I/O suite: 8/8 passed, 52.40s, zero failed/ignored/filtered.
- Bootstrap: Java/Node-free Runtime/Macro reproduction twice, shipped artifact comparison, identity verification and 4/4 execution tests passed (13.93s); zero failures/ignores/filters.
- Production repair focus: frontend 7/7 (0.27s); native host 18/18 (52.25s); event loop 7/7; native I/O 8/8 (36.78s); PTY 1/1 (7.15s); persistent REPL 8/8 (43.47s). These also execute within the passing unfiltered baseline. Unit selectors filter unrelated tests and are not whole-package execution.
- Independent full-diff, repair, fixture and original-criterion reviews: no unresolved material findings or missing original criterion. Author repairs were reviewed independently. The full-baseline pass supports the implementation criteria; final-head CI and promotion remain separate required gates.

Commands, failures, interrupted obsolete runs and terminal logs are preserved in
[handoff](handoff.md). Interrupted baselines and prior-head green checks are not
final acceptance. The workspace baseline also retains pre-existing, individually
explained ignores: 37 legacy prototype cases awaiting compiled support, three
manual evidence-recording commands, and one pre-existing compiler documentation
example. None is counted as passing or bypasses an original M3 acceptance criterion. These are not counted as passing tests;
no ignore attribute is added by this PR. The active
`prototype_cases_awaiting_compiled_support` guard checks recorded unsupported
compilation diagnostics and fails when they change or compilation succeeds;
it does not establish semantic support.
Original M3 acceptance regressions execute rather than being skipped. The first two CI runs exposed fixture assumptions, insufficient
fuel sweep horizons and Linux SIGINT races; assertions were retained while the
setups and production observer/idle-dispatch behavior were repaired.

## Contract boundaries and precision

The native Unix frontend has one Store owner and explicit pending I/O producers.
Pure interrupted observations retain roots and wait for signal acknowledgment.
The dispatch probe skips scheduler service only after proving it unnecessary;
terminal dependency, cancellation/yield, actionable stream service and native
retirement work keep dispatch required. Blocked reads and backpressured writes
retain their roots without unnecessary service. Cancellation-preflight interruptions
retry after input acknowledgment, preserving reset/exit intent and buffered Source.
Classification clears before cancellation mutation; uncertain mutation stays fatal.
The getter tests inject interruptions after successful real observations and prove
classification/control flow rather than an exact external-signal timing window. Interrupted post-submit tracking retains the completed result and retries
observation without replaying Source. Uncertain ownerless
mutation/teardown remains quarantined. Ordinary language errors recover.

Compiled source streams provide bounded chunk reads/writes, backpressure, single
endpoint ownership, explicit EOF distinct from nil, close/failure and cancellation.
Read/write readiness transitions use in-place injected snapshot states, not
production snapshot publication. The frontend withdrawal-journal fixture explicitly establishes an unpublished
prepared phase-2 state and proves idle detection requires dispatch. Separately,
`later_published_write_receipt_survives_earlier_read_service_or_retirement`
constructs a published transfer-receipt fixture after a genuinely fuel-interrupted
owner and proves service/retirement ordering. Neither claims to witness the exact
publication fuel window.

The production canonical mapping supports one async u32 import/export. Executing
component-to-component tests deliver cancellation event 6, retain awaited cleanup,
and release imports, transfer allocations and driver resources before acknowledgment.
The handwritten caller WAT and compiled Wasm fixtures are tracked and required.
Host resources have explicit teardown; GC finalization is not their correctness
mechanism. Raw embedders are responsible for equivalent lifetime/quarantine handling.

Complete upstream analyzer schemas, published project dependencies, generic nested
WIT future/stream interoperability and non-Unix interactive async I/O certification
retain later work scopes. These limits do not remove an original #12–#15 criterion.
The 10,000-task test proves sequential task retirement and actual heap recovery,
not a generic WIT or OS-resource stress claim. Existing unassessed compatibility
inventory entries remain unassessed. The broader constructor corpus uses 100M fuel
and has a separate default-budget regression.
