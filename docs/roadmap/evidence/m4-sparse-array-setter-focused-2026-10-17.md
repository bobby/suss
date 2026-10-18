# Sparse-array setter focused evidence

Tested source commit `290e20eb8fa925d3e00faf99def0d4b7388aa684` is ae28 plus the two reviewed source files and four normally regenerated BOOT artifacts. The setter reuses its complete first validation and performs one ordered sparse search; validation coverage and write order are retained.

ae28 exact focused results: 33 PASS, 2 FAIL, 0 ignored; 35 unique tests. The unchanged three-test collection-source-asts binary exit is 101. Raw individual outcomes and commands are archived. This is not a full baseline or a claim that prior acceptance fuel failures are resolved.

The ce6 contextual run is a different tuple: 15 sparse tests passed; the original nine-local route failed with typed fuel exhaustion before coherent/effect assertions. Do not combine its counts or BOOT with ae28. No full performance or callback-criterion success is inferred.

Full source freezes, six exact captured source/BOOT blobs, approved source snapshots, raw bootstrap/test logs and available terminal reviews are retained. The accompanying `m4-sparse-array-setter-git-binding-2026-10-17.json` verifies all 4,041 frozen inputs against this source commit, expanding the pinned ClojureScript gitlink. Raw archived packaging notes predate the source commit and retain their pending-binding wording; this separate binding supplies the completed verification.

Commands:
```text
cargo run --profile test -p suss-cli --bin suss-bootstrap --locked -- runtime/bootstrap
cargo test -p suss-compile --locked --test runtime_abi sparse_set_repair_tests:: -- --test-threads=1 --nocapture
cargo test -p suss-compile --locked --test runtime_abi -- runtime_abi_sparse_array_length_presence_and_uint32_boundary_survive_gc runtime_abi_sparse_array_shrink_deletes_indices_without_resurrection_or_length_change_on_delete runtime_abi_sparse_array_rejects_malformed_length_instead_of_saturating runtime_abi_sparse_array_rejects_corrupt_chains_before_shrink_mutates_length runtime_abi_sparse_array_bulk_ingress_copies_input_and_preserves_explicit_undefined runtime_abi_sparse_array_dense_prefix_presence_and_shrink_remain_independent runtime_abi_sparse_array_rejects_duplicate_out_of_range_and_dense_overlap_keys runtime_abi_sparse_array_mutations_reject_invalid_presence_without_repairing_it runtime_abi_sparse_array_materialization_is_linear_snapshot_and_keeps_sparse_length_independent runtime_abi_source_array_sparse_owner_clone_preserves_holes_and_maximum_length_after_gc runtime_abi_sparse_array_slice_preserves_holes_dense_offsets_and_high_sparse_ranges runtime_abi_sparse_clone_coalesces_present_prefix_without_filling_holes_or_aliasing --test-threads=1 --nocapture
cargo test -p suss-cli --locked --test core_import -- --test-threads=1 --nocapture
cargo test -p suss-cli --locked --test compiled_macro_collection_source_asts -- --test-threads=1 --nocapture
```

No quota, tariff, callback, or test-source edits beyond the three new regressions; no merge/closure/readiness assertion.

Observed collection failures (both actual all-fuel-consumed diagnostics):

- `collection_source_nodes_retain_original_children_before_factory_lowering`: line231, span851..882, compiled macro invocation.
- `unordered_literal_ast_edges_and_runtime_effects_preserve_textual_order`: line291, span510..531, caller &env input transport.
- `empty_do_source_return_retains_context_and_nil_after_gc`: PASS.

Archive SHA256: `974124ec9c17176039f145bbadc85a3070a7bde7622d5607316fd4e835a6b723`. Compiler and bootstrap bytes in the publication commit remain exactly those of the tested source commit above. No final-head CI or new full-baseline result is claimed.
