# Snapshot-owned objects retain their intrinsic type

Tested source: `a504e95f57dec0d3f1d58bbdb1ea943a8148fa17`, on the separate coherent/shared-witness candidate. Production Draft #268 remains010e8d1; no adoption of this candidate is implied.

Each snapshot node already owns the exact captured GC object. Incoming root/edge references must compare identical to those owned references before they can match. A legal Wasm GC object cannot change its intrinsic heap type, struct field count or array length. Storing an additional StructType/ArrayType wrapper alongside each node therefore adds no distinguishing capability to this exact-reference predicate. This proof does not apply to shape-compatible replacement objects, mutable nominal descriptors, source facts, prototype state or authenticated memo data.

The node retains its Struct/Array kind and all original atoms. Capture keeps fallible type queries and full field/element reads. Validation keeps fallible kind queries, fresh struct type/field-count comparison, array type query/length and every atom/slot-hook check. Three additional struct count controls are charged before the performed work. Source, nominal/prototype, memo, foreign/reset, exact reference and callback checks are unchanged. Node allocation/relocation and retirement use actual smaller record sizes; numeric rates/quotas are unchanged. No new cache, numeric type-ID authority or guest storage verdict is introduced.

Independent review verifies the invariant against locked Wasmtime49.0.1 primary source: Store host-type registrations and module registry retain the necessary metadata while live objects/Store remain. An initial design note cited installed49.0.3; the four relevant primary implementation files are byte-identical to49.0.1, and the final source review binds the actual locked version. No dependency version changed. Existing nested Drop accounting limitations remain; this does not close #252 generally.

The new regression drops temporary caller host type and guest Module/Instance/function handles before capture, forces GC, rejects same-type/different-reference and same-data/different-array-type replacements, observes field/element mutations as false, restores them and verifies post-GC matches. The full21 snapshot tests pass, retaining scalar bits/functions, aliases/cycles, strict/cut/slot/type/owner/IRQ/fuel/promotion and cleanup tests. The complete unchanged source-child fixture passes. Coherent/legacy18 still return12PASS/6fuelFAIL; both original public identity tests still fuel-fail. Across42 unique selected Rust tests:34PASS/8FAIL/0ignored. The standalone lifetime test repeats a member of the21 and adds no unique count. Blocked callback/poison/identity assertions remain unexecuted; do not infer their acceptance from these partial guards.

Measured Node size falls136→88bytes; Contents is32, StorageSnapshot80 and Atom48 on this host. All six default-budget callable requests complete51 source-root constructions rather than49. The exact ordered first49 cost210,422–215,421 fewer units, including the new count charges. They still fail constructing boundary26 before export construction or final certificate validation. This is matched completed-prefix progress, not total request cost, a passing identity contract or a global performance claim. The separate failed atom-alignment trial has no such saving and is not part of this source.

Normal bootstrap and compiler identity pass. All3662 tracked pre-run file hashes remain unchanged at each producer stage; five captured source/bootstrap files matcha504e95 exactly. Raw native/public/guard failures, source manifest, commands, matching control, locked-version/source and evidence reviews are archived. No fresh upstream CLJS/Jank oracle, full workspace baseline, new final-head CI, production transplant, issue closure, draft promotion or merge is claimed.

Commands use the shared bounded target plus `CARGO_BUILD_JOBS=1 RAYON_NUM_THREADS=1`, without RUSTFLAGS assignment:

```text
cargo run --profile test --locked -p suss-cli --bin suss-bootstrap -- runtime/bootstrap
python3 scripts/verify_bootstrap_identity.py
cargo test --locked -p suss-compile --lib native_storage_snapshot_owned_nodes_survive_external_type_and_module_drop -- --test-threads=1 --nocapture
cargo test --locked -p suss-compile --lib storage_snapshot::tests:: -- --test-threads=1 --nocapture
cargo test --locked -p suss-compile --lib source_child_certificates_reject_wrong_owner_equal_doc_and_forged_parent_memo -- --test-threads=1 --nocapture
cargo test --locked -p suss-compile --lib -- source_roots::coherent::tests:: legacy_cache_retirement_is_atomic_and_not_sustained_by_its_own_native_references --test-threads=1 --nocapture
cargo test --locked -p suss-cli --test compiled_macro_namespace_identity resolved_callable_info_keeps_raw_declaration_children_across_macro_invocations -- --exact --test-threads=1 --nocapture
cargo test --locked -p suss-cli --test compiled_macro_namespace_identity compiler_namespace_and_declaration_aliases_match_pinned_five_call_observations -- --exact --test-threads=1 --nocapture
```

Exact argv/output/trace environment is retained in run.py. Next: review the source-bound terminal evidence and validate any isolated production transplant against its own source/bootstrap tuple; continue remaining construction work and unchanged public criteria. The full required workspace baseline and passing final-head CI remain necessary before any promotion.

[Raw source/controls, logs, manifests, scripts and independent reviews](m4-snapshot-owned-types-2026-10-18.tar.gz), SHA256 `eecf0316755029a9361e6ec3eaa9b73086735b3a09901dad936dc6d04f3770ec`; 34 members/33 payloads.
