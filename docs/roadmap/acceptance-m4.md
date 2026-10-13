# M4 acceptance work record

## Original-criterion source audit — 2026-10-12

This section supersedes the older implementation-gap descriptions below, but
does not certify current-head acceptance. The original bodies of
[#16](https://github.com/bobby/suss/issues/16),
[#17](https://github.com/bobby/suss/issues/17),
[#18](https://github.com/bobby/suss/issues/18) and
[#19](https://github.com/bobby/suss/issues/19) were reread from GitHub; all four
remain open. Scope includes the public collection contracts in design section 4,
including record behavior. This audit was authored under the user's test hold.
The user subsequently authorized focused validation followed by the full baseline;
current execution evidence is recorded in the latest handoff entries. The full
baseline and original-criterion completion audit remain pending.

| Original criterion | Authoritative source coverage located | Remaining acceptance evidence/work |
| --- | --- | --- |
| #16 notices and hashes | [Import gate](../../scripts/verify-core-import.sh), retained extracted originals, EPL notice, manifest and explicit patch hashes. | Previous gate results are recorded in handoff; reproduce the current published snapshot before closure. |
| #16 source updates invalidate reviews | [Review validator regressions](../../scripts/test_cljs_reviews.py) and source hashes in the reviewed inventory. | Retain stale-hash rejection evidence with the final source snapshot. |
| #16 portable forms load in Suss namespaces | [Executable import tests](../../crates/suss-cli/tests/core_import.rs), reproducible runtime/macro bootstrap. | Import and bootstrap execution gates remain required after final changes. |
| #16 adaptations are explicit | [Import recipe](../compatibility/core-import.json), [review overlay](../compatibility/reviews.edn), whole-form patches and dependency ledgers. | Complete provenance for any further record imports; extraction alone is not execution. |
| #17 31/32/33 and deeper boundaries | [Vector tests](../../crates/suss-cli/tests/compiled_macro_vectors.rs), pinned vector corpus, 1057→1056→1057 collapse/regrowth regression. | Reconcile final runtime/macro results with pinned observations; previous vector results do not validate new observation code. |
| #17 sharing and metadata | Vector backing/root inspection, [Subvec tests](../../crates/suss-cli/tests/portable_subvector.rs), retained old versions across GC. | Current-head execution and identity evidence remain required. |
| #17 lazy/chunk effects | [Map/filter corpus and native tests](../../crates/suss-cli/tests/portable_lazy_transformations.rs), [chunk-effect tests](../../crates/suss-cli/tests/portable_chunk_effects.rs), demand boundaries and retry traces. | Execute new non-forcing decoder/harness regressions and rerun affected differential evidence when the test hold is lifted. |
| #18 collision/deletion and nil/false keys | [HAMT tests](../../crates/suss-cli/tests/compiled_macro_hash_maps.rs), pinned hash-map corpus, collision deletion and dense-node collapse tests. | Final executed results are required; source coverage is not a new passing result. |
| #18 comparators and iteration | [Sorted-tree tests](../../crates/suss-cli/tests/portable_sorted_tree_collections.rs), raw comparator/sharing probes, cursor/range observations; [FIFO queue tests](../../crates/suss-cli/tests/portable_queues.rs). | Preserve comparator-defined sorted order, FIFO queue order and unspecified hash-map/set traversal. Current-head gates remain pending. |
| #18 cross-type equality and nominal records | Pinned array-map/hash-map equality/hash cases; existing record helper/iterator/type-context prerequisites. | **Public `defrecord` construction, factories and collection behavior remain missing.** Hand-written IRecord/IEquiv fixtures would not establish the public contract. Implement and execute the complete adapted pinned record macro/helper graph, then test same-type equality, different record types, records versus maps and persistence. |
| #19 equal values hash equally | [Collection hash tests](../../crates/suss-cli/tests/portable_collection_hash.rs), HAMT cross-type equality/hash cases, sorted collection corpus. | Include actual public records and preserve equality/hash agreement across their operations. |
| #19 metadata survives required operations | Vector/Subvec, HAMT/set, sorted-tree and queue corpora, lazy/chunk metadata tests. | Record metadata operations are still part of the missing public record implementation. |
| #19 transient invalidation errors | [Exact transient error tests](../../crates/suss-cli/tests/portable_transient_errors.rs), map/set/vector lifecycle regressions, captured lookup after persistence. | Execute the unchanged required lifecycle tests at the final source snapshot. |
| #19 Reduced stops before later effects | [Reduction boundary tests](../../crates/suss-cli/tests/portable_reduction.rs), 108 pinned vector reduce/reduce-kv cases, lazy/chunk and transducer stop/throw traces. | Audit all implemented collection reduction paths, including public records, against the full original scope. |
| #19 structural sharing is checked | Independent vector/tree child identity, Subvec backing identity, HAMT old roots and queue front/rear storage across GC. | Keep executing raw identity checks; public record persistent updates still need implementation and evidence. |

The original proposed `collections_sequences`, `collections_associative` and
`collections_contract` filters are acceptance targets, not proof that a matching
test suite currently exists. Use the actual focused binaries above and confirm
their nonzero executed case counts. The required final workspace command remains
`cargo test --workspace --locked -- --test-threads=2`; no fuel, flags or test
settings are changed by this audit.

The latest recorded full workspace run failed with four out-of-fuel traps in
`generated_async_import_bridge`. Those failures remain unresolved; their cause
must be established rather than assuming they are independent of import/bootstrap
changes. PR [#234](https://github.com/bobby/suss/pull/234) remains draft at remote
head `77b904087f712176f610e4aa74e8ea120c032829`; GitHub reports failed integration
shards 1, 2 and 4 and the aggregate test check. Its head predates the current
uncommitted fixes. A passing known-failure corpus baseline does not establish M4
compatibility or permit closure of #16–#19.

Next implementation task: finish the full pinned `defrecord` macro/helper
adaptations and positional/map factories, preserving notices and hashes. Cover
nominal equality/hash, metadata, persistent assoc/dissoc, extension fields and
retained values across GC with public record construction. When test execution
is authorized again, run focused observation/collection checks first, then import,
bootstrap, the required full baseline and final-head CI; reconcile every criterion
before promotion or closure.

Record dependency source follow-up (2026-10-12): the runtime import now includes
the complete pinned replace/take/Iterate graph used by emit-defrecord's generated
field substitutions. The [record source record](../compatibility/record-macro-foundations/README.md#runtime-field-substitution-dependencies--2026-10-12)
links the adaptations, originals and three authored two-phase native regressions.
Generation wrote 433 files; no native/pinned execution occurred. Shipped
bootstrap now predates this source and requires regeneration before any validation
can establish a new result. This advances an actual macro dependency without
removing the public record construction/factory gap or completing #18/#19.

Record intrinsic source follow-up (2026-10-12): low-level defrecord*/deftype* body
lowering is authored, with hidden record fields, declared-field counts, genuine
catalog record flags and method field binding. Two authored two-phase compiler
boundary regressions are linked from the record source record above. They remain
uncompiled/unexecuted and do not establish public generated record semantics.
Public compiled macro/helper/factory installation and complete record acceptance
remain open; neither the original criteria nor the test hold changed.

Compiled factory source follow-up (2026-10-12): the complete positional/map
factory helper adaptations are now installed by the macro bootstrap source and
included in macro artifact dependency identities. Two native tests and four
provenance regression methods are authored but unexecuted. The existing import
gate now covers the new copied helper source identities and loader/cache wiring.
Full public record expansion and genuine record-input conversion still require
implementation/execution; this is not closure evidence for #18/#19 or a new
passing validation result.

Emitter dependency/hash-label follow-up (2026-10-12): six complete sequence/
transducer helper forms and compiled scalar type-label source are authored with
provenance and pending native/provenance regressions. Runtime generation wrote
439 files; no executed acceptance result changed. These remove dependencies of
the full emitter and its name-based hash seed without certifying record behavior.
The public emitter remains the next implementation task. Clarification of the
earlier test hold after cleanup/reactivation is pending; do not infer permission
to execute from elapsed time.

## Bounded MLIR evidence audit after merged #227/#228 — 2026-10-09

This section supersedes older pending execution status for #188 only. It does
not remove or complete any criterion of original M4 issues #16–#19.

The [post-#228 audit](../../experiments/mlir/evidence/post228/README.md) maps all
seven bounded #188 criteria to actual WasmGC/source/effect/GC/ABI observations,
route assessment, measurements, risks and the published deferral recommendation.
Nine execution gates passed at `b82ad94`; its exact-head workspace baseline
passed 1,550 tests, zero failures and 41 existing ignores. All ten final-head CI
checks succeeded before PR #227 was merged as `f4b13f3`. All 184 experiment files,
120 production files and four bootstrap artifacts matched that executed snapshot
before the evidence documentation edit. Historical logs/receipts remain intact.

The evaluation recommends **deferral**, not compiler adoption. Root CI does not
run the isolated bridge or C++ checks, so their independent execution logs and
identity manifest are retained. This documentation checkpoint still needs its
own independent review, required workspace baseline and final-head CI before
promotion. Original M4 extraction/sequence/associative/collection-contract issues
remain open; sorted collections, nominal records and full acceptance are not
proved by this bounded compiler investigation.



## Current execution snapshot after merged #226 — 2026-10-08

This snapshot supersedes the publication/pending status in the older work record
below. Original issues #16–#19 remain open; no original acceptance criterion is
removed. PRs #222, #223, #225 and #226 are incorporated in main `4e0f00d`.

| Requirement | Current evidence | Remaining proof/work |
| --- | --- | --- |
| #16 complete notices, hashes, invalidation, namespace loading and explicit adaptations | Current map/filter draft preserves all 325 main recipe selections and adds six complete pinned forms; 335 artifacts reproduce, 432 reviews validate. Whole-form patches, EPL packaging and actual pre-import failures are retained. | Bootstrap regeneration and four two-phase native tests pass; fresh byte reproducibility/import/full-baseline gates remain pending. Maintain full collection dependency provenance as sorted/record work lands. Extraction alone is not compatibility. |
| #17 vector/list/lazy/chunk values, effects, metadata and sharing | Published vector, Subvec, chunk and lazy prerequisites remain on main. Complete map/filter source now has 41 fresh pinned typed observations including demand boundaries, all arities, shortest inputs, throw/retry, live helpers, ordered effects and repeated filter chunk reads. Original 24 full-source CLI probes pass and printed values match. | All four native tests pass after regeneration (14.63s, zero failures/ignores/filters): complete corpus, fragmented captures/cache across GC, sole host handles and independent raw backing-array identity. Final publication and complete original #17 acceptance remain open. All 17 additional CLI probes passed matching printed values; these remain supplemental. |
| #18 associative collisions/deletion, nil/false keys, comparators, iteration, equality and records | Published HAMT/array-map prerequisites remain on main. Queue #226 passed exact-head full baseline (1,546 passed, zero failed, 41 existing ignores), independent review and all final-head CI before promotion/user merge; three native tests include raw sharing/GC and iterator contracts. | Full portable comparators and sorted collections, nominal record construction/equality/hash/metadata and original associative acceptance remain implementation work. Protocol-only fixtures do not prove sorted collection behavior. |
| #19 hash/equality, metadata, transient invalidation, Reduced effects and sharing | Existing published focused suites remain evidence for their covered operations. New map/filter primary cases establish transducer Reduced identity and callback/reducer throw ordering; raw sharing tests are authored. | Four new two-phase native tests pass; finish the full contract across remaining sorted/record/reduction operations and final publication gates. Passing partial corpora do not establish the complete contract. |
| Separate #188 bounded MLIR evaluation and decision | Committed experiment executes actual WasmGC, shared ABI/rooted closures, source preservation and effect/cleanup examples at the retained `22723b4` base; decision recommends deferral. Recovered source rebases onto `4e0f00d` without changing experiment or reused production source. | Four bootstrap identities changed. Restore pinned tools and rerun affected native/source/effect/evidence gates and required full baseline/final-head review/CI. Cleanup-interrupted baseline51551 exited101 with its log removed; it is not a pass. No adoption decision is made. |

Map/filter focused native execution now passes. Required final gates retain the existing
focused-first procedure and finite test allowances; no RUSTFLAGS override:

```sh
cargo test -p suss-cli --test portable_lazy_transformations --locked -- --test-threads=2
scripts/verify-core-import.sh
scripts/verify-bootstrap.sh
cargo test --workspace --locked -- --test-threads=2
```

Independent reviews found no material source/test defect in the 41-case draft,
bootstrap regeneration and four native tests have now passed; final-head review and publication gates remain required. The four oracle verifier
regressions reject concealed or mistyped evidence; these test the harness rather
than certify guest execution. M5–M7 remain outside this goal.

## Earlier work record

Scope is the original criteria in issues #16–#19 and the separate bounded MLIR
evaluation #188. This is a work record, not milestone completion evidence.

| Criterion | Current evidence and remaining work |
| --- | --- |
| #16: notices, hashes, stale-review invalidation, portable namespace loading, explicit adaptations | PR #222 repairs loader lexical boundaries and includes standalone sequence provenance in the gate. Importer 15 tests, standalone provenance seven tests and native import 17 tests pass. 323 artifacts reproduce; 420 reviews validate, 645 declarations remain unassessed. Exact-head full baseline passed: 1,531 passed, zero failed/filtered, 41 existing ignores, 201 suites. All ten CI checks passed; independent final review found no material issues. PR #222 is ready for Bobby’s review. |
| #17: vector boundaries, structural sharing and metadata | Existing 31/32/33 and deeper growth tests; new 1057→1056→1057 collapse/regrowth tests execute in both Runtime and Macro phases. All 91 fresh pinned vector observations match, preserving the original 40. Complete native suite: eight passed, zero failed/ignored/filtered. Deep equality exceeds the original 100M test budget; measured cost justifies the corpus-only 500M allowance. Revised-head full baseline passed: 1,532 passed, zero failed/filtered, 41 existing ignores, 201 suites. All ten final-head CI checks passed; PR #223 is ready for Bobby’s review. |
| #17: lazy/chunked effect contract | Nine fresh pinned chunk-effect traces match. Consolidated native chunk tests: two passed; existing lazy tests: four passed. These use an explicit public chunk producer; full public map/filter compatibility remains unproven. |
| #18: collisions, deletion and nil/false keys | All 111 fresh pinned HAMT observations match, including collision deletion, dense-node packing, nil/false conversion and bidirectional array/hash-map equality and hashing. Consolidated native suite: eight passed. This is partial #18 evidence. |
| #18: comparator iteration, cross-type equality and nominal records | Existing HAMT iteration is insufficient for comparator order. Sorted collections and nominal record execution remain implementation work; explicit array-map/hash-map bidirectional equality/hash evidence is needed. |
| #19: hashing, metadata, transients, Reduced and sharing | Existing suites cover many cases; public structural-transition sharing and vector reduce/reduce-kv callback termination at chunk boundaries need focused executing checks. |
| #188: bounded MLIR evaluation | Checksum-verified LLVM 23.1.3 macOS ARM64 SDK runs and contains wasmssa. Registered custom dialect, executable WasmGC bridge, source-analysis transport, effects/cleanup, comparison measurements and decision record remain required. SDK acquisition alone proves none of those semantics. |

The vector fixture uses public collection operations for pinned comparisons.
Private native probes check actual trie shifts and object sharing. The retained
original handle is decoded after GC while its global also remains rooted; this
is not a sole-handle lifetime claim. Runtime and Macro observations must execute
before this work can be called accepted.

Each PR requires independent review, the required unfiltered workspace baseline
and green final-head CI before promotion for Bobby's review. Merging is a
separate decision. Keep future M5–M7 work outside this record.

## Current combined-lane prerequisite

The consolidated focused run passed HAMT 8, chunk 2, lazy 4 and reduction 3 tests, but failed all three Subvec tests on the fixture’s unresolved defn. The shared helper was repaired to def/fn, preserving all 45 pinned observations; the rerun passed two Subvec tests and failed the public corpus on unresolved empty. The complete upstream empty form is now imported with source provenance, 324-file reproduction, 421 reviewed/644 unassessed overlay validation and 15 importer tests passing. Bootstrap regeneration is running in an isolated target directory after a shared-target dependency-identity compilation failure. Empty native tests and the final combined rerun remain pending. Neither setup failures nor authored tests count as acceptance.

Combined regenerated-bootstrap native gate completed exit 0: HAMT 8, chunk 2, empty 3, lazy 4, reduction 3, Subvec 3, exact transient errors 1 = 24 passed; zero failures/ignores/filters across seven suites. Command and log `/private/tmp/suss-m4-collection-combined-second.log` preserve the complete selected targets. This supersedes earlier Subvec unresolved defn/empty failures. Fresh transient oracle confirmation, independent final combined review, unfiltered full workspace baseline and final-head CI remain pending; no original issue is closed by this partial gate.
