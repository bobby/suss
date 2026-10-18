# Boxed construction facts: coherent-candidate measurement

Tested source: `f9c8bc1`, based on the separate coherent/shared-witness experiment `9552257`. The bounded change is also being tested independently on Draft #268's production base; those source/bootstrap tuples and outcomes are separate.

Construction records now store boxed array/constructor input owners, preserving their original rooted values, schema, field inventories and watches. Small scalar/seed records no longer reserve and relocate the largest inline payload. The helper prepays twice the actual payload size (allocation plus move) and four bounded Box initialization/release controls before allocation. Record pages and pending deferred-factory vectors still charge their actual capacities and live relocation bytes. Existing variable-sized nested teardown policy remains; this is not a claim that all historical Drop work is metered or that allocator OOM is recoverable. No numeric rate, budget, callback, authority, validation, source fixture or page-ordinal policy changes.

Normal/deferred production factories use the charged helper before domain enrollment/publication. The new regression uses unboxed prepared owners and sweeps each host-fuel cut plus active interruption for both payload kinds and caller phases. Failed unpublished owners release their handles; prior33record prefix survives, GC validation and retry succeed. Existing page-publication cancellation tests remain separate. Mechanical fixture-only Box adjustments preserve old test setup paths; those setups do not measure the new helper fee.

Normal bootstrap and compiler identity pass. The new cut test passes standalone and again inside the25-test ownership/deferred/domain/seed cohort. The cohort passes25/25, zero ignored; the standalone repeat adds no unique coverage. Observed Fact size is24bytes on this host. Both unchanged original public identity tests still fail at default allowances: callable has six fuel errors/no rows; namespace fails its first lexical call in both phases. The selected unique Rust total is25PASS/2fuelFAIL/0ignored across27tests.

All six callable requests now complete49source-root constructions rather than45. For the identical ordered first45boundaries, enclosing construction debits fall from8,267,281–8,317,641 to7,672,475–7,722,813 units:594,802–594,828units removed, including new Box helper costs. All six now fail during boundary1 construction rather than boundary23. No coherent certificate finalization is reached. This is a matched completed-prefix improvement, not total request cost, a successful public identity contract, or a whole-workspace performance result. Box fees sit outside narrower ledger/native-factory trace labels, so those labels alone cannot establish net savings.

Independent reviews cover source ownership/accounting and exact matched-prefix reconstruction from raw logs. Five changed Rust files are byte-identical to the bounded transplant selected for #268, whose five preimages also match the candidate base. No coherent/shared-witness code is implied by that transplant. The nine captured source/bootstrap files bind exactly to f9c8bc1. All3660tracked pre-run hashes remain unchanged through each candidate stage; gitlinks/external development-oracle trees are not a whole-input freeze claim. The initial E0499 check failure and corrected compile success are retained.

Commands use CARGO_TARGET_DIR=/home/bobby/.cache/suss-m4-work/m4-cost-diagnostic-target CARGO_BUILD_JOBS=1 RAYON_NUM_THREADS=1; no RUSTFLAGS assignment:

```text
cargo check --locked -p suss-compile --tests
cargo run --profile test --locked -p suss-cli --bin suss-bootstrap -- runtime/bootstrap
python3 scripts/verify_bootstrap_identity.py
cargo test --locked -p suss-compile --lib boxed_fact_cuts_ -- --test-threads=1 --nocapture
cargo test --locked -p suss-compile --lib -- construction_records::tests:: deferred_factory::tests:: write_domain::tests:: canonical_seeds::tests:: --test-threads=1 --nocapture
cargo test --locked -p suss-cli --test compiled_macro_namespace_identity resolved_callable_info_keeps_raw_declaration_children_across_macro_invocations -- --exact --test-threads=1 --nocapture
cargo test --locked -p suss-cli --test compiled_macro_namespace_identity compiler_namespace_and_declaration_aliases_match_pinned_five_call_observations -- --exact --test-threads=1 --nocapture
```

The exact trace/output environment and terminal commands are retained in run.py/receipt.json. Coherent guards, fresh pinned CLJS/Jank oracles, full workspace baseline and final-head CI are unexecuted for this candidate. No new passing public criterion, issue closure, coherent adoption or promotion. Next: complete the bounded production transplant's focused gates and publish it to existing Draft268; continue source-domain capture work on this candidate afterward, without a new branch.

[Raw source/bootstrap, controls, logs, receipts, scripts and independent reviews](m4-boxed-construction-facts-2026-10-18.tar.gz), SHA256 `75536d01ee91023fc51256e11b78cc0334a823296584b38bd1d9da4338ee78df`; 36 members/35 payloads.
