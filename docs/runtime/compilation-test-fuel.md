# Compilation fuel for semantic integration tests

The compilation allowance for these tests is independent of Runtime evaluation
limits and shipped defaults. Issue #252 permits calibration so correctness tests
can execute their complete programs. Explicit exhaustion, cancellation, recovery
and default-budget tests keep their own limits.

The initial common allowance is **10,000,000,000 units per outer compilation
operation**, supplied through `CompiledMacros::set_operation_fuel`. Nested work
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
