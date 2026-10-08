# M4 acceptance work record

Scope is the original criteria in issues #16–#19 and the separate bounded MLIR
evaluation #188. This is a work record, not milestone completion evidence.

| Criterion | Current evidence and remaining work |
| --- | --- |
| #16: notices, hashes, stale-review invalidation, portable namespace loading, explicit adaptations | PR #222 repairs loader lexical boundaries and includes standalone sequence provenance in the gate. Importer 15 tests, standalone provenance seven tests and native import 17 tests pass. 323 artifacts reproduce; 420 reviews validate, 645 declarations remain unassessed. Full baseline and updated-head CI are pending. |
| #17: vector boundaries, structural sharing and metadata | Existing 31/32/33 and deeper growth tests; new 1057→1056→1057 collapse/regrowth tests are prepared in both Runtime and Macro phases. All 91 fresh pinned vector observations match, preserving the original 40; native execution remains pending. |
| #17: lazy/chunked effect contract | Existing lazy realization/retry coverage; effect traces distinguishing within-chunk realization from crossing into the next chunk remain to be added and executed. |
| #18: collisions, deletion and nil/false keys | Existing HAMT/collision tests; deletion to singleton/empty, dense-node packing and combined nil/false discrimination through conversion need focused evidence. |
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
