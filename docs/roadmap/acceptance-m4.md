# M4 acceptance work record

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
