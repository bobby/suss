# Coherent retention: original public identity probes

This experiment combines the existing coherent-retention candidate c015779 with production b846d86. Source integration commit `0f7c81a98bfc51aac8505d40726fcc3bcff70cee` preserves the sparse-array setter and compact capture/generation changes. The sole Rust merge conflict combines NonZeroUsize and Weak imports; the generation-discovery loop also uses the coherent controller's prepaid stack-population helper. Independent source review verifies these merge details. This remains separate from Drafts #263/#268 because the actual public requests regress from decoded rows to fuel failures.

The unchanged production callable probe on b846 executes three requests per caller phase and decodes all six rows. The first row matches in each phase; calls two and three lose raw-declaration identity and saved raw/resolved method-params identity. There are four mismatched rows, twelve false identity positions, and no transport errors. This is a semantic failure, not a passing baseline. The raw log and JSON are retained separately from candidate evidence. The original inline expected observations and budgets are unchanged; no new pinned ClojureScript or Jank run is claimed here.

On integration source 0f7c81a, normal bootstrap regeneration and compiler identity verification pass. All four capture-index regressions pass. Both original public CLI tests execute unchanged with default allowances and fail: the callable probe has six caller-&env fuel errors and no decoded rows; the five-call namespace probe fails at its first lexical call, span46..82, in both phases before final rows. All 3,651 frozen file hashes stay unchanged through execution and resolve exactly to the tested Git commit.

Admission traces isolate a concrete performed cost. In the first callable request, completed native expectation-projection intervals consume 8,249,382 fuel; witness-validation intervals consume 617,105. The next projection, boundary2, starts with675,586 remaining and fails before its after marker. Outer expectation intervals overlap these inner intervals and must not be added. These are native admission costs, before environment construction and guest macro invocation.

Source `b1bfd86b4136c8c9c1fe8a8e77b05cbdcf80e4f1` changes only the projection child-buffer growth policy and regenerates normal bootstrap artifacts. After the first successful child, the buffer reserves known small arity up to64 slots, avoiding repeated allocation/relocation. All-declined inputs allocate no Form buffer; large collections retain bounded geometric growth. Actual requested allocation and live moved bytes remain prepaid at the same tariffs. This is an allocation strategy, not a form limit. Sparse-success/later-decline inputs can pay more upfront than before; resource-failure positions may change. This policy has source-only independent review, not approval for production adoption.

Three unchanged native differential tests pass on this second source: exact projected Forms for44 real source roots plus their export; complete/declined/malformed siblings and metadata; and fuel/IRQ cuts, retry, repeated aliases and bounded diamond exhaustion. Across45 separate native operations, the preserved original algorithm costs6,500,173 units and the current algorithm costs1,678,385. This comparison includes earlier projection improvements and does not isolate buffer growth or establish a complete-request saving.

The original public callable probe still fails all six requests during caller-&env transport, with zero decoded rows. Admission now completes, leaving266,026–270,403 of the default10M allowance; construction/validation then exhausts fuel. The namespace probe is NOT rerun on b1bfd86, so its current semantic results remain unexecuted. Its earlier failure is a separate source tuple. The other coherent-retention guards and full issue acceptance remain unexecuted on this source.

The existing reader-before-every-source-alias regression is executed on b1bfd86 and fails with fuel exhaustion in its ORIGINAL-route control at coherent_tests.rs1450. The coherent arm and callback value/count/order comparison remain unexecuted. An initial short --exact filter selected zero tests; its raw log is preserved and adds no coverage. The corrected filter selects exactly one test. All 3,651 frozen inputs remain unchanged through the corrected execution and match the Git source commit.

Per-source execution counts are separate: 0f7c81a has4PASS/2fuelFAIL; b1bfd86 has3PASS/2fuelFAIL, counting its callable and corrected reader tests. The repeated callable test is not extra unique coverage. Production b846's semantic failure is separate. No original identity criterion is established by these runs. No whole workspace baseline, new oracle execution, CI pass, issue closure, PR promotion or merge is claimed.

Commands use CARGO_TARGET_DIR=/home/bobby/.cache/suss-m4-work/m4-cost-diagnostic-target, CARGO_BUILD_JOBS=1 and RAYON_NUM_THREADS=1, without assigning RUSTFLAGS. Full argv, environment overrides and terminal receipts are archived. Key commands:

```text
cargo run --profile test --locked -p suss-cli --bin suss-bootstrap -- runtime/bootstrap
python3 scripts/verify_bootstrap_identity.py
cargo test --locked -p suss-compile --lib source_roots::index_tests -- --test-threads=1 --nocapture
cargo test --locked -p suss-compile --lib private_project_ -- --test-threads=1 --nocapture
cargo test --locked -p suss-cli --test compiled_macro_namespace_identity resolved_callable_info_keeps_raw_declaration_children_across_macro_invocations -- --exact --test-threads=1 --nocapture
cargo test --locked -p suss-cli --test compiled_macro_namespace_identity compiler_namespace_and_declaration_aliases_match_pinned_five_call_observations -- --exact --test-threads=1 --nocapture
cargo test --locked -p suss-compile --lib coherent_reader_before_every_source_alias_preserves_original_effect_order -- --test-threads=1 --nocapture
```

Identity drivers set SUSS_NAMESPACE_IDENTITY_OUTPUT, SUSS_METADATA_FUEL_TRACE=1 and SUSS_MACRO_FUEL_TRACE=1. The production before run uses only the JSON output option; the corrected reader test uses metadata trace only. Native source/BOOT snapshots and a patch against b846 are archived for each source. The merged capture-index test's old full-clear control has a different first-pass stack-population implementation; its layout regressions pass, but its costs are not a reset-only comparison.

Next work must reduce repeated expansion of immutable native source witnesses across captured roots and exports, preserving complete source/storage/prototype/memo validation and callback order. A shared native witness DAG is a direction for review, not an implemented repair or a guest-validation cache. The existing candidate must remain separate until the original public probes and mutation/callback guards execute successfully.

[Raw logs, JSON, source/bootstrap, manifests, drivers and reviews](m4-coherent-identity-acceptance-2026-10-18.tar.gz). SHA256 `25056b6529c5d7176ca2a115c72c99acc934a0aeffdc78d2e09b02ab619b2166`; 63 members / 62 payloads, all bound by MANIFEST digests. The included shared-native-witness design review describes obligations for a future implementation; it is not adoption approval.
