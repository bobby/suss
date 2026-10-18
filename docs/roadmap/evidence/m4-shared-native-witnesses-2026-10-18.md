# Shared immutable native source witnesses

Tested source: `d781cdb2175ed0cbbc14add613f7da6431ffeff8`, based on the separate coherent-retention experiment. It is backed up on `experiment/m4-coherent-identity-acceptance`; production Drafts #263/#268 do not adopt this candidate.

The projector now shares completed immutable native syntax by exact recipe ID within one finalized graph. It borrows the frozen recipe slice, tracks active/completed IDs with metered reachable storage, and discards private memo state on errors. Equal-content distinct recipe IDs are not interned. Metadata overlays share an immutable body and retain independent metadata; malformed structure, unsupported projection and ambiguous keys remain separate outcomes. Native nodes own copied syntax rather than compiler-owner Arcs or guest handles.

Logical witnesses retain current finite guest decoding, source-validity checks and independent comparisons. Small unordered data uses paired comparison; wide data feeds canonical expected child IDs into the existing raw-key encoding and compares against an independent actual plan. No expanded Form is reconstructed in production admission/comparison. A test-only conversion checks exact projected data, spans, metadata, scalar bits and UTF16. Source-owner/child ordinals, weak leases, live storage, constructors, prototypes, authenticated memos and final publication remain separately checked. Private placement was extracted without changing its traversal or checks so Rust can keep recipes immutably borrowed through admission.

Independent source reviews found two initially missing performed-work charges: pair staging and per-node encoding-scratch lifecycle. Both were restored before final bootstrap and execution. Source reviews and the committed-head binding are archived; neither constitutes issue acceptance.

Normal bootstrap regeneration and compiler-identity verification pass. Five new native differential tests pass: all44 real captured roots plus export, malformed/declined siblings and metadata replacement, exact-ID sharing, small/wide unordered comparison against independent plans, and selected typed fuel/IRQ cuts with retry preserving prior roots. The previous45 separate native projections cost1,678,385 units; all45 now project in one default-budget operation for196,791, including shared lookup/allocation work. This native proxy removes1,481,594 units but does not measure a complete macro request.

Both unchanged original public CLI probes execute at their existing default allowances and still fail. Callable has six caller-&env fuel errors and zero decoded rows. Namespace fails its first lexical call, span46..82, in both caller phases; later namespace/identity/value/effect observations remain unexecuted. The callable admission marker now retains9,224,441–9,228,014 fuel, compared with266,026–270,403 in the preserved b1bfd86 control. The observed prefix difference is8,957,611–8,958,415, not total required request fuel or proof of resolved identity. Construction or certification after admission still exhausts the operation budget.

The coherent module executes17 guards:11PASS/6fuelFAIL. A separate legacy-cache retirement test passes, making the full18-test guard cohort12PASS/6fuelFAIL. Native5 plus the guard cohort and two public tests total **17PASS/8fuelFAIL/0ignored across25 selected Rust tests**. Every failure and all blocked assertions remain visible. Four coherent tests fail during initial build/materialization before their later intended checks. Both reader-effect comparison tests fail in their ORIGINAL-route control before the coherent arm or effect comparison.

The historical17-test cohort includes16 coherent tests plus legacy retirement and reports12PASS/5fuelFAIL. All its failed names remain, and no historically passing name is now failed. The additional reader-before-every-alias case already failed in the b1bfd86 control. These are different source/cohort snapshots; this audit is not a matched causal regression experiment. Public b846's decoded-but-mismatched callable rows remain separate historical evidence; this experimental branch still regresses in execution reach relative to that production control.

All3,656 frozen acceptance inputs remain unchanged and resolve exactly to d781cdb; all13 captured source/bootstrap files match that commit. The initial native5 run has reviewed source-hash/Git correspondence but no separate whole-tree before-run freeze. Preliminary failed compiler checks and an earlier bootstrap are retained as exploratory history, not final-source verification. The verified final bootstrap is bootstrap-v2. Native5, public tests, guards and legacy retirement are distinct selected tests, not repeated additive coverage.

Commands use CARGO_TARGET_DIR=/home/bobby/.cache/suss-m4-work/m4-cost-diagnostic-target, CARGO_BUILD_JOBS=1 and RAYON_NUM_THREADS=1; no RUSTFLAGS assignment. The acceptance driver records exact argv, trace/output options, terminal results and unchanged-input checks.

```text
cargo run --profile test --locked -p suss-cli --bin suss-bootstrap -- runtime/bootstrap
python3 scripts/verify_bootstrap_identity.py
cargo test --locked -p suss-compile --lib shared_witness_ -- --test-threads=1 --nocapture
cargo test --locked -p suss-cli --test compiled_macro_namespace_identity resolved_callable_info_keeps_raw_declaration_children_across_macro_invocations -- --exact --test-threads=1 --nocapture
cargo test --locked -p suss-cli --test compiled_macro_namespace_identity compiler_namespace_and_declaration_aliases_match_pinned_five_call_observations -- --exact --test-threads=1 --nocapture
cargo test --locked -p suss-compile --lib source_roots::coherent::tests:: -- --test-threads=1 --nocapture
cargo test --locked -p suss-compile --lib legacy_cache_retirement_is_atomic_and_not_sustained_by_its_own_native_references -- --test-threads=1 --nocapture
```

No new pinned Jank/CLJS oracle run, full workspace baseline, final-head CI pass, issue closure, PR promotion or merge is claimed. Those remain required after the focused criteria pass. The next unblocked action is to distinguish constructor work from certificate finalization within the unchanged failing requests and remove repeated performed work while retaining callback and source/storage authority checks. The native admission bottleneck is reduced; the public contract is not established.

[Raw source/bootstrap, manifests, logs, JSON, controls, driver and independent reviews](m4-shared-native-witnesses-2026-10-18.tar.gz), SHA256 `e7ac1214ca1d2251e6e312c64e80f49d37d2ad79a2d8e8bfdece8239886cbbd2`;42 members/41 payloads with MANIFEST digests.
