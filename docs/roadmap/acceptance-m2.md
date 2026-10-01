# M2 compiler and runtime foundation acceptance audit

Status: candidate audit, 2026-10-01. The roadmap remains in progress until
independent review verifies every criterion and exact reviewed-head CI passes.
This evaluates the foundation work packages against their published criteria;
it does not claim M3 compiled macros/REPL, M4 complete collections, or any release
gate. Closing links must be added only after this audit is independently accepted.

## M2-01 / issue #8: reader, metadata and namespace phases

| Published criterion | Implementation and executed evidence |
| --- | --- |
| Diagnostics locate forms | Lossless `suss-reader::forms` stores byte spans and ordered prefix metadata; portable HIR retains both. `portable_pipeline::hir_retains_binding_identity_metadata_and_located_diagnostics` and `portable_resolution::namespace_globals_lower_into_shared_binding_reads` assert exact locations/metadata. Native malformed-source and missing-class tests assert compile errors before initialization/publication. |
| .sus/.cljs/.cljc ambiguity fails | `portable_resolution::namespace_sources_reject_extension_and_root_ambiguity` rejects extension and root collisions; `portable_modules` executes immutable graph preparation and located dependency errors. |
| :suss/:cljs order matches contract | `forms::resolve_conditionals` takes the first matching clause in source order; reader conditional regressions exercise matching feature reversal, nested selection, splicing and metadata target validation. |
| Phase imports are tested | `namespace_phases_resolve_and_execute_distinct_cells` compiles actual Runtime/Macro fragments using separate dependency aliases/refers and imported live cells, observing11/22; Runtime cannot resolve Macro-only imports. This is compiler phase resolution, not compiled macro bootstrap or source :require-macros acceptance. |
| cljs.core aliases are tested | `namespace_core_aliases_import_one_canonical_live_cell` inspects actual module imports for one suss.core/shared cell and executes both names as42 after GC. Alias/refers/exclusions tests execute qualified and renamed arithmetic bindings. |

Focused reader and namespace audits39472 completed successfully. The full
workspace root baseline28575 executes these guards and completed through reader
doctests. Fourteen pinned scalar reader observations are bounded scalar evidence,
not a complete portable core claim. Source :require-macros remains rejected with
an explicit diagnostic pending the isolated compiled macro session in M3-03;
this audit must not describe those source directives as supported.

## M2-02 / issue #9: explicit evaluation-order IR

| Published criterion | Implementation and executed evidence |
| --- | --- |
| Calls evaluate once in order | `portable_closures::calls_evaluate_computed_callee_then_each_argument_exactly_once` validates and executes independently generated fragments with observed callee/argument traces. Non-callable and dynamic arity checks preserve argument effects and language recovery. |
| Conditions evaluate once in order | `portable_pipeline::arithmetic_import_trace_observes_once_only_source_order_and_short_circuit` executes typed shared-ABI imports and asserts trace/short-circuit behavior; HIR lowers control through explicit values/blocks. |
| Collection entries evaluate once in order | PR#114 introduces vector/map/set HIR paths using captured factory calls and ordered large-map entry temporaries. Six native guards and19 independently decoded observations cover both threshold paths, entry effects, throw stopping later entries/construction, lookup capture, GC and diagnostics. Native constructor-interface fixtures establish compiler evaluation behavior, not persistent collection algorithms. |
| Dispatch evaluates once in order | `persistent_session::nominal_method_receiver_fields_and_callable_fields_preserve_recur_and_evaluation_order`, native missing-method/receiver tests and `portable_object_methods` guards exercise captured method lookup, argument effects and live protocol cells across fragments. |
| Recur evaluates once in order | Verified explicit block edges replace parameters in parallel; `portable_pipeline::edge_parameter_replacements_are_parallel_in_executed_ir`, source loop/function regressions and native protocol receiver-changing recur exercise actual artifacts. Lexical/tail/arity/cross-function negative tests reject malformed targets with spans. |
| Wrong arity and unresolved types are diagnostics | `portable_closures::source_wrong_arity_and_unimplemented_signatures_are_located_diagnostics`, malformed HIR/IR tests and `persistent_session::nominal_unknown_types_preserve_order_and_failed_analysis_does_not_publish_bindings` assert located compile errors. Dynamic wrong arity returns a typed language exception instead of a Wasm trap. |
| Old emitter reevaluation is removed | `codegen::generate_condition_inner` emits the operand once then inspects a scratch local. Legacy executing `condition_evaluates_effect_once`, `condition_preserves_falsey_and_truthy_effects`, `nested_conditions_preserve_effect_order`, `comparison_arguments_evaluate_once_even_when_false` and `arithmetic_and_equality_preserve_argument_effect_order` guard the earlier repairs. Reviewer must check the emitter itself, not infer this criterion from a green count. |

The 2026-10-01 accepted decision explicitly preserves textual set ordering and
map key/value interleaving at all sizes. Exact pinned results remain separate:
19 observations contain15 shared values and4 deliberate variance observations,
0 skips; no matching-compatibility claim is made for the four differing results.
The original17 source probes are retained with explicit per-runtime expectations.

Unsupported keyword/symbol expression literals, quoted collections and runtime
literal metadata remain documented compiler/core work; reader metadata stays in
HIR. M4 must provide real persistent collection classes/factories. The legacy
CLI/AOT path has not migrated and the M1 diagnostic corpus's known failures
remain explicit; M2 foundation artifacts must not be presented as release or
production frontend acceptance.

## M2-03 / issue #10: shared ABI and closures

Issue#10 was closed by merged acceptance PR#64; its criterion-by-criterion
[evidence](acceptance-runtime-abi-v1.md) remains applicable to the foundation.
The accepted 2026-10-01 ABI2 decision adds lazy identity storage without changing
existing field offsets or universal invocation. Actual ABI/prelude mismatch
regressions still reject incompatible fragments before initialization; owned
roots, cross-fragment closures and foreign-runtime diagnostics execute in the
full workspace. ABI2 artifacts must be rebuilt; ABI1 is not claimed compatible.

## M2-04 / issue #11: nominal identity, protocols and exceptions

| Published criterion | Implementation and executed evidence |
| --- | --- |
| Same-layout types remain distinct | `persistent_session::nominal_same_layout_types_keep_distinct_identity_through_aliases_and_gc` creates same-field descriptors, checks all positive/negative instance relations and retains exact i31 observations after GC. Type redefinition tests preserve old descriptor/constructor identities. |
| Protocols extend across fragments | `nominal_protocol_extensions_update_existing_objects_across_fragments` declares objects before extension, publishes a later fragment and observes changed dispatch on the original retained object after GC. Native captured-dispatcher and namespace extension guards exercise current tables and direct/specific/default precedence. |
| try/catch/finally preserves behavior | Ten `portable_exceptions` native tests cover raw payloads, typed descriptor catches,1234 cleanup order, superseding exceptions, captured bindings, nested regions, failed initializers and recovery. Public HIR/IR negatives reject malformed regions and invalid recur crossings. |
| Dynamic bindings preserve behavior | Ten `portable_dynamic_bindings` native tests cover parallel initializers, current-value snapshots, old closures, nested restoration, throw/finally, rooted thrown closures and exhaustive fuel-boundary cleanup recovery. |
| Unknown types fail | `nominal_unknown_types_preserve_order_and_failed_analysis_does_not_publish_bindings` rejects UnknownType and malformed declarations with source spans and unchanged cell counts; later reads prove no leaked declarations. |

Root full28575 executes the relevant nominal/protocol/exception/binding guards.
Complete public Error/printing/stack interfaces, runtime metadata, compiled
macros, asynchronous dynamic context/cleanup and complete portable core remain
later acceptance work. None is hidden by an ignore or inferred from test counts.

## Review and readiness gates

Root source headc7c9c0b and documentation-only5a58ba1 have fresh focused5/native19,
Python86, inventory1065, core-import115 and review-overlay206/859 evidence. Full
root28575 completed with exit0 through final reader doctests. PR#114 independent
review added a constructor/EMPTY_NODE capture regression and completed full8004
with exit0 through final reader doctests. Its reviewed final head is
`ddf00b21372982fca7b79c216d988de67da033b4`; fresh19 reference/native observations
and six native tests pass. Exact independently reviewed head CI is still required.

This candidate audit changes no implementation and closes no issue. Reviewers
must identify any criterion lacking actual acceptance evidence and keep that
issue open; remaining unsupported features must not be silently relabeled as
acceptance successes. GitHub merge and automatic issue closure remain under the
user's authority. Future M3–M9 work stays open.


The runnable acceptance command is `sh scripts/test-m2-foundation.sh`. It executes
reader, portable compiler/runtime, legacy operand-order and native session suites
explicitly; unlike the originally proposed `evaluation_order` name filter, it
does not silently select zero tests. Run with the shared target and two build
workers. This focused command does not replace the required
`cargo test --workspace --locked -- --test-threads=2` or fresh pinned oracle runs.
