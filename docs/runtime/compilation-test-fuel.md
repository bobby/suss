# Compilation fuel for semantic integration tests

The compilation allowance for these tests is independent of Runtime evaluation
limits and shipped defaults. Issue #252 permits calibration so correctness tests
can execute their complete programs. Explicit exhaustion, cancellation, recovery
and default-budget tests keep their own limits.

The initial common allowance is **10,000,000,000 units per outer compilation
operation**, supplied through `CompiledMacros::set_operation_fuel` or the public
Compiler's explicit compilation-fuel variants. Nested work
continues to consume that same cumulative budget; there is no retry/escalation.
The completed core-environment measurement on source `19defbf` used at most
2,630,830,814 units for one traced expansion. The 10B allowance gives that
representative workload headroom; it is not a measured maximum for all programs.

Increasing the allowance does not accelerate compilation or bound total host/JIT
time or memory. Semantic failures must still be reported. Historical low-budget
failures and the source-bound measurements remain in the existing evidence files.

The following settings replace the earlier compilation allowances. Caller Session
settings, sources, metadata, expected values/effects and assertions are retained.

| Test | Previous compilation allowance | New allowance |
| --- | ---: | ---: |
| `actual_protocol_indexes_preserve_caller_phase_and_own_identity` | 10,000,000 | 10,000,000,000 |
| `compact_membership_uses_real_bindings_and_failed_staging_restores_mode` | 10,000,000 | 10,000,000,000 |
| `field_reference_asts_match_pinned_flags_tags_positions_and_identity` | 10,000,000 | 10,000,000,000 |
| `compiled_source_macros_materialize_env_only_for_macros_that_can_read_it` | 10,000,000 | 10,000,000,000 |
| `compiled_source_macros_pass_env_through_renamed_and_rewritten_helper_expansions` | 10,000,000 | 10,000,000,000 |
| `quote_and_literal_child_records_match_pinned_without_datum_analysis` | 10,000,000 | 10,000,000,000 |
| `compiled_function_declarations_match_primary_presence_and_parameter_shapes` | 10,000,000 | 10,000,000,000 |
| `completed_declaration_hint_boundaries_match_actual_primary_expression_tags` | 10,000,000 | 10,000,000,000 |
| `public_compiler_macros_receive_real_lexical_bindings_and_runtime_effects_do_not_replay` | 10,000,000 | 10,000,000,000 |
| `compiled_defn_preserves_docs_attributes_arities_and_lexical_shadowing` | 10,000,000 | 10,000,000,000 |
| `compiled_macro_artifact_retains_runtime_state_and_roots` | 10,000,000 | 10,000,000,000 |
| `public_reduce_dispatch_and_source_order_match_pinned_cljs_after_gc` | 10,000,000 | 10,000,000,000 |
| `captured_global_declarations_supply_var_ast_fields_without_catalog_mutation` | 10,000,000 | 10,000,000,000 |
| `declaration_metadata_merge_and_tag_precedence_match_pinned_analyzer` | 10,000,000 | 10,000,000,000 |
| `compiled_declaration_records_match_actual_primary_snapshot_and_catalog_fields` | 100,000,000 | 10,000,000,000 |
| `compiled_wrapper_serializes_only_the_genuine_native_object_fact_in_both_phases` | 10,000,000 | 10,000,000,000 |
| `compiler_catalog_policy_is_opt_in_and_invalid_metadata_keeps_old_macro` | 100,000,000 | 10,000,000,000 |
| `failed_script_staging_restores_callable_and_catalog_policy_after_gc` | 100,000,000 | 10,000,000,000 |
| `failed_macro_reload_keeps_published_policy_then_successful_reload_recovers` | 100,000,000 | 10,000,000,000 |
| `source_macro_consumes_actual_other_namespace_and_protocol_revisions_after_gc` | 100,000,000 | 10,000,000,000 |
| `compiled_macro_core_namespace_keeps_complete_declaration_records` | 10,000,000 | 10,000,000,000 |
| `control_source_nodes_match_genuine_primary_children_and_forms_after_gc` | 40,000,000 | 10,000,000,000 |
| `compiled_source_inference_matches_pinned_tags_without_changing_executed_storage_or_effects` | 10,000,000 | 10,000,000,000 |
| `collection_source_nodes_retain_original_children_before_factory_lowering` | 40,000,000 | 10,000,000,000 |
| `unordered_literal_ast_edges_and_runtime_effects_preserve_textual_order` | 10,000,000 | 10,000,000,000 |
| `public_record_field_and_metadata_values_decode_independently_after_gc` | 10,000,000 | 10,000,000,000 |
| `public_record_custom_protocols_and_record_input_factory_decode_after_gc` | 10,000,000 | 10,000,000,000 |
| `public_record_nominal_equality_and_predicate_decode_after_gc_in_both_phases` | 10,000,000 | 10,000,000,000 |
| `constructor_source_nodes_match_pinned_children_tags_and_class_facts_after_gc` | 100,000,000 | 10,000,000,000 |
| `public_records_execute_jank_and_authored_contracts_with_two_nominal_classes` | 10,000,000 | 10,000,000,000 |

The public-record command supplies `suss compile --compile-fuel`; subsequent
`suss run` invocations retain their existing Runtime settings.

The pinned map harness retains its existing 200M allowance. These changes do not
alter default-fit controls in the same files or the dedicated cumulative-fuel suite.
Validation commands and observed results are recorded in the handoff.


The new `compiled_macro_large_finite` positive fixture separately uses100M for
constructing and reading its returned Runtime value, and10B for compilation.
Its first4097-element case compiled at10B but exhausted the new fixture's initial
10M evaluation allowance. At explicit100M evaluation, all four cases in both
caller phases complete with full data/metadata/GC checks. This changes no
pre-existing Runtime regression limit or shipped default. There is no automatic
retry/escalation in the test; the original10M failure remains recorded in evidence.


The six additional protocol/field/source/quote cases above exhausted Macro fuel
in actual4baf CI (five caller-environment transports and one transformer
invocation). Their compiler-only settings are now calibrated, with every other
test byte and caller setting preserved. Their four complete targets now pass
10 tests on the recorded pre-observation-scan source/bootstrap snapshot.
The function-declaration target also passes both tests, and the source-hint
target passes its original 23 observations in both phases. These are focused
results; final-source workspace and CI validation remain separate gates.

The three public Compiler cases above also have recorded default-fuel failures:
two in integration0 on2caae8c, and one unchanged expression-API probe on that
same source. Only their compilation calls use the new allowance; all original
Runtime settings, programs, state/GC observations and assertions remain.
Their two complete targets now pass 25 tests, including new None/zero/preflight,
recovery and actual artifact execution controls. The reduction fixture also had
a recorded &env fuel failure on that source; its complete four-test target passes
with the same compiler-only policy and unchanged pinned observations. These
focused results do not establish the required workspace or final-head CI gates.


The global-reference fixture has a separately retained old-head integration7
&env fuel failure586..621. Only that fixture now uses the same10B compilation
allowance; the full two-test target passes with all11 oracle cases, revision/effect
checks, Runtime settings and GC assertions retained. The other case keeps its
original compiler allowance. Whole-workspace and final-head CI gates remain.
