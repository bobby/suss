# Complete CoreEnv diagnostic after private-slot token scan repair

Tested/published production head: `19defbfe57c506d58e36d6a9ae58cc71707654cc` (code payload commit `12b88bcecf5e300360745c67ded9820f8afd9fc3`). Control: `c59b6237a2eb9fe6bf353304cd1efe0ac2e40901`. This packet records a completed diagnostic plus a separate unchanged pinned-map execution. It changes no production source, bootstrap artifact, default allowance or reset policy.

The controller first ran the original pinned map test with its original 200M harness allowance, then temporarily added the same explicit 10B Macro-only setter as the c59 control to the complete original CoreEnv fixture. Both caller phases, caller defaults, all four requests and every semantic assertion remained byte-exact after reversing that one insertion. There was no Graph observer or bootstrap regeneration. The controller completed, drained its children and restored the sole owned CLI test; no extra execution was performed by the reviewer.

## Actual canonical expansion costs

| Caller / request | c59 | 19def | Saved units |
|---|---:|---:|---:|
| Runtime / identity | 1,299,517,125 | 1,299,517,125 | 0 |
| Runtime / graph-owned | 3,766,083,473 | 2,630,830,814 | 1,135,252,659 |
| Macro / identity | 1,299,585,605 | 1,299,587,037 | **−1,432** |
| Macro / graph-owned | 3,766,081,893 | 2,630,829,234 | 1,135,252,659 |
| Four traced expansion subtotal | 10,131,268,096 | 7,860,764,210 | **2,270,503,886 (22.4108558%)** |

Each second request removes exactly1,135,252,659 observed units relative to its matched control. The Runtime first request is unchanged. The Macro first request is adverse by1,432 units; its cause is not established and the raw difference is retained. All four expansion checkpoint chains reached decoded, and the original whole diagnostic test passed1/0/0. This subtotal covers the four traced macro expansions, not all setup/native JIT/compilation work or a whole-program cost measure. No failed tail remains unknown in these four completed chains.

The independent raw reparse verified every checkpoint and arithmetic, separate from the main comparison JSON. The test harness reports637.42s versus the control's694.26s; driver durations include compilation/setup and are retained separately. These are single uncontrolled runs, **not a benchmark or an attributed wall-clock speedup**.

## Original pinned map

The separate current-head map test passed1/0/0 at its unchanged200M harness allowance. The captured sequence contains all69 original IDs in exact order. The unchanged independent host comparator yields62 host passes and7 guest-judged assertions, no failures/skips/missing/unexecuted assertions or namespace failure. The seven guest judgments are not relabelled independent host passes. This demonstrates this original declaration's transport/execution, not blanket collection conformance or default10M transport.

## Source and artifact limits

All3,553 pre-overlay frozen paths bind to actual Git blobs at19def, with the CLJS gitlink verified separately. Before-first-Cargo differs only by the one owned CLI test. Both stage freezes are identical; source/bootstrap bytes are unchanged throughout. Final restored freeze equals the pre-overlay freeze. Both complete tests' original fixtures are the same Git blobs across c59 and19def; the only crate production difference is private_fixups/slot_log.rs plus its four generated bootstrap artifacts. Full recorded input bindings are archived. The earlier five-payload source/bootstrap evidence is referenced by published Git path and SHA rather than duplicating its Wasm archive.

The archive is Git-resolved evidence, **not a standalone whole-source repository**. It retains both raw complete logs/receipts/freezes and diagnostic-only test bytes, actual capture and oracle reference/subset, unchanged comparator source, reproducible parse/audit code and independent review. Its manifest maps logical names to content-addressed safe regular SHA256 payloads. No live source or freshly regenerated bootstrap is substituted for the tested bytes.

## Interpretation and next decision

The implemented token scan repair materially reduces the second requests but leaves first-request construction at about1.299B. This does not establish unchanged default10M CoreEnv acceptance. The three previously observed original inference/CoreEnv/collection fuel failures are not waived by the diagnostic. No full baseline, final-head CI, readiness, promotion, issue closure or merge is claimed. The normal unit matrix and its T1/T2 fixed+2/+1 overhead remain in the separate source evidence; they are not added to these two integration test executions.

Budget/reset policy remains an explicit user decision. A separately prepared `--compile-fuel` configuration change may expose an independent compiler allowance without loosening Runtime evaluation or changing either default; it is not part of this tested head or packet. Continue exact original acceptance and required baseline/final CI under the agreed policy, without disguising configured diagnostic success as default acceptance.

## Commands

Commands/environment are retained verbatim in run.py/receipt.json: `cargo test --locked -p suss-compile --test clojure_test_suite pinned_map_declaration_executes_without_transport_loss -- --exact --test-threads=1 --nocapture`, then `cargo test --locked -p suss-cli --test compiled_macro_core_environment compiled_macro_core_namespace_keeps_complete_declaration_records -- --exact --test-threads=1 --nocapture`. Target/jobs/Rayon1 and macro trace are recorded; no RUSTFLAGS override. No bootstrap command was executed in this diagnostic. audit.py verifies Git bindings/overlay and derives costs; compare-map.py preserves the original independent comparator. replay.py validates the packaged manifest/payloads and rechecks cost/map data without Cargo or the source worktree.

[Raw complete-cost evidence](m4-pr256-private-slot-complete-cost-2026-10-18.tar.gz) — SHA256 `a86eb92841d6bfde6294299a107137addbed92ff36f88c1ec9f7f713b962c23b`. The archive has44 safe regular members,43 content-addressed payloads and56 logical file bindings.
