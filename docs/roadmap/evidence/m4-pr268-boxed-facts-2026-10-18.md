# Boxed construction facts on Draft #268

Tested source: `014e0f0d4072e36e6a3c9bb4e47dfcaa8553dc7f`, based on production `b846d86`. Only five Rust files and four normally regenerated bootstrap files change. The five Rust preimages match the separate coherent candidate's base exactly, and their afterimages match its reviewed `f9c8bc1` implementation. This transplant contains no coherent retention, shared native witnesses, native-field tables or experimental profiling code.

Large array/constructor fact owners now live in separately metered boxes, leaving small scalar/seed ledger records compact. Helpers charge actual payload allocation and movement plus bounded Box lifecycle controls before allocation. Page and deferred-vector growth still charge actual backing and live relocation sizes. Inputs move intact: receiver, schema, fields, watches, domain links, callbacks and every validation remain. Existing variable nested teardown policy is unchanged; allocator OOM is not made recoverable. New Box fees precede enrollment/publication and remain inside enclosing request budgets. No quota or numeric rate changes.

Normal bootstrap regeneration and compiler identity pass. All 3,635 tracked pre-run file hashes remain unchanged through every stage. That includes all 251 tracked Jank vendor files; the vendor is a repository tree, not a Git submodule. The initial vendor-layout inspection incorrectly compared a parent commit to a subtree ID; its raw receipt and corrected tree/hash check are retained. Nine captured source/bootstrap files match the tested source commit exactly.

The focused selection executes **57 unique Rust tests: 53 PASS, 4 FAIL, 0 ignored**:

| Selection | Pass | Fail | Evidence |
| --- | ---: | ---: | --- |
| Ownership, records, deferred factories, domains and seeds | 25 | 0 | Includes new both-phase/both-payload fuel-cut/IRQ cleanup, prior-record preservation, GC and retry; existing mutation/method/foreign checks remain. |
| Capture indexes and generations | 4 | 0 | Complete DAG, rollover and recovery guards retained. |
| Complete source-child fixture | 1 | 0 | All five variants in both phases and physical/memo negative tail; all 220 ordered events match b846. |
| Core imports | 17 | 0 | All unchanged tests use existing defaults. |
| Public namespace and callable identity | 0 | 2 | Both decode their full observations but fail identity comparisons. |
| Collection source nodes | 1 | 2 | Projection and unordered-literal requests still exhaust fuel. |
| Pinned CLJS observations and decoder checks | 4 | 0 | Four pinned value/getter-effect cases match after GC in both phases within one Rust test; three decoder adversaries also pass. |
| Original pinned Jank map | 1 | 0 | All 69 ordered assertion IDs execute; independent comparison reports 62 host passes and 7 guest judgments, no failure/missing/skip. |

After the complete source-child fixture, each phase retains 508,962 units rather than b846's 155,874: 353,088 units removed from that original outer request at its unchanged 10M allowance. Both executions already pass; this is additional budget margin, not a newly resolved criterion or a general performance claim. The five coverage variants, ordered events and results match exactly after ignoring balances. The separate coherent candidate's roughly 595K matched-prefix result is not production validation and is not added to this figure.

Callable decodes six rows with four mismatched rows and twelve identity positions, exactly matching b846's complete saved receipt. Namespace decodes ten rows, all mismatched, with 52 false-versus-true identity differences per phase. The latter has no exact b846 namespace control in this packet; do not claim the allocation patch caused new execution reach or a new semantic regression. Values, membership and identity assertions remain unchanged. These defects stay under #264; no identity repair is established here.

Collection projection still fails during macro invocation at851..882 with its existing 40M allowance. Unordered literals still fail during caller-&env input transport at510..531 with default10M. Their later required values/effects remain unexecuted. The map test retains its existing 200M allowances, complete original source and independent ordered-ID comparison; it does not establish default-budget map execution or whole-suite conformance. Existing pinned CLJS references are used; no new upstream JVM/Node oracle execution is claimed.

Commands use CARGO_TARGET_DIR=/home/bobby/.cache/suss-m4-work/m4-cost-diagnostic-target, CARGO_BUILD_JOBS=1 and RAYON_NUM_THREADS=1, without setting RUSTFLAGS:

```text
cargo run --profile test --locked -p suss-cli --bin suss-bootstrap -- runtime/bootstrap
python3 scripts/verify_bootstrap_identity.py
cargo test --locked -p suss-compile --lib -- construction_records::tests:: deferred_factory::tests:: write_domain::tests:: canonical_seeds::tests:: --test-threads=1 --nocapture
cargo test --locked -p suss-compile --lib source_roots::index_tests -- --test-threads=1 --nocapture
cargo test --locked -p suss-compile --lib portable_macro_graph::source_roots::tests::source_child_certificates_reject_wrong_owner_equal_doc_and_forged_parent_memo -- --exact --test-threads=1 --nocapture
cargo test --locked -p suss-cli --test core_import -- --test-threads=1 --nocapture
cargo test --locked -p suss-cli --test compiled_macro_namespace_identity -- --test-threads=1 --nocapture
cargo test --locked -p suss-cli --test compiled_macro_collection_source_asts -- --test-threads=1 --nocapture
cargo test --locked -p suss-cli --test portable_short_property_observations -- --test-threads=1 --nocapture
cargo test --locked -p suss-compile --test clojure_test_suite pinned_map_declaration_executes_without_transport_loss -- --exact --test-threads=1 --nocapture
```

The driver retains exact output/trace environment and independent map comparison. Public JSON receipts are recovered from their existing log markers because the shared output file contains only the last probe. Source, committed-source and focused evidence reviews are retained. The full `cargo test --workspace --locked -- --test-threads=2` baseline remains unexecuted while focused acceptance fails; final-head CI must run after publication. No #270/#252/#264/#241 closure, promotion, merge or M4 completion is claimed. Next: publish this bounded update to existing Draft268, inspect its exact-head CI and continue the unresolved identity/collection transport work with unchanged criteria.

[Raw source/bootstrap, controls, logs, receipts, parsers, manifests and independent reviews](m4-pr268-boxed-facts-2026-10-18.tar.gz), SHA256 `da18bf23b1c632ceefd113f4f0ca43a8181bdf1b614e01735d4870bc680b6596`; 44 members/43 payloads.
