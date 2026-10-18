# Owned-object storage snapshots on Draft #268

Tested source: `36b4d07a37d685a18b6aba81f767ffc58fe13823`, based on `010e8d100f98060daf106536c8f7d637ea77c9bb`. One Rust file and four normally regenerated bootstrap artifacts change. The Rust afterimage matches the independently reviewed `a504e95` candidate; coherent retention/shared witnesses and generated native-field tables remain separate.

Snapshot nodes already retain the exact original GC objects, and incoming root/edge references require exact reference identity. An object's intrinsic Wasm heap type cannot change. Remove duplicate persistent StructType/ArrayType wrappers while retaining fallible kind/type queries, fresh struct field counts, array lengths, every field/element/atom check, source facts, mutable nominal descriptors/prototypes, slot hooks and authenticated memo validation. Three added struct-count controls are prepaid; no quota or numeric tariff is reduced. On this host Node shrinks from136 to88 bytes; Atom remains48. This does not introduce a new cache or numeric type-ID authority.

The new regression drops caller type/module/instance/function handles before capture, runs GC, rejects equal-content replacement objects and different array types, and checks mutation/restoration of host/guest structs and arrays. The locked Wasmtime version is49.0.1; its Store owns the necessary registrations. The earlier source-proof inspection of49.0.3 was corrected: the four relevant implementation files have identical bytes in49.0.1. No unsupported unsafe heap corruption is claimed as covered.

Normal bootstrap regeneration and compiler identity verification pass. All3637 tracked pre-run file hashes match the tested Git commit and stay unchanged through every stage. Focused selection executes **78 unique Rust tests:74PASS/4FAIL/0ignored**:

| Selection | Pass | Fail |
| --- | ---: | ---: |
| Storage snapshots, including external-handle lifetime regression |21|0|
| Ownership, records, deferred factories, domains and seeds |25|0|
| Capture indexes and generations |4|0|
| Complete source-child fixture |1|0|
| Core imports |17|0|
| Public namespace/callable identity |0|2|
| Collection source nodes |1|2|
| Pinned CLJS value/effect and decoder checks |4|0|
| Original pinned Jank map |1|0|

Both public JSON receipts exactly match the saved010e receipts: callable decodes six rows with twelve false-versus-true identity positions, and namespace decodes ten rows with104 identity mismatches. No identity repair is established. Collection projection still exhausts during macro invocation851..882 at its existing40M allowance; unordered literals still exhaust during caller-&env transport510..531 at default10M. Later assertions remain unexecuted. No previously passing selected test fails.

The complete source-child fixture retains all five variants/both phases, physical/memo negatives and220 ordered events. It leaves502185 rather than508962 fuel per phase: **6777 more units spent**, about0.0714% of the former debit, in each unchanged10M request. This is an adverse whole-fixture tradeoff, retained alongside the representation benefit. The separate coherent candidate's constructor-prefix savings are a different source/workload; they are not production performance evidence and are not subtracted from this overhead.

The original pinned Jank map retains all69 ordered assertion IDs, independently compared:62 host passes/seven guest judgments/no failure,missing,skip, at its existing200M allowances. This does not establish default-budget execution or whole-suite conformance. Four pinned ClojureScript value/getter-effect cases match afterGC in both phases within one Rust test; three decoder adversaries also pass. Existing pinned observations are used; no new upstream oracle run is claimed.

Exact argv and trace/output overrides are in run.py and receipt.json. Environment: CARGO_TARGET_DIR=/home/bobby/.cache/suss-m4-work/m4-cost-diagnostic-target, CARGO_BUILD_JOBS=1, RAYON_NUM_THREADS=1, inherited RUSTFLAGS. Bootstrap command: cargo run --profile test --locked -p suss-cli --bin suss-bootstrap -- runtime/bootstrap. Compiler identity: python3 scripts/verify_bootstrap_identity.py. Focused Cargo commands use --locked and --test-threads=1 --nocapture; selectors and complete raw output are archived without hiding failed stages.

The required full baseline `cargo test --workspace --locked -- --test-threads=2` remains unexecuted on this head while focused acceptance fails. Previous010e CI failed (foundation541 Rust passes/one existing ignore plus353 Python passes; integrations111passes/14fuel failures;229of259 integration targets unexecuted). It is not current-head CI evidence. Require independent final-head review and new CI after publication. No issue closure, promotion, merge or M4 completion. Next: update existing Draft268, retain exact-head CI outcomes, and repair actual identity/collection fuel acceptance under unchanged contracts.

[Raw source/bootstrap, controls, logs, receipts, parsers, manifests and independent reviews](m4-pr268-owned-types-2026-10-18.tar.gz), SHA256 `3d7d9790b84797f7b1e0c954842d94aceedb2583d55ef3839047b368aed2e1a6`; 44 members/43 payloads.
