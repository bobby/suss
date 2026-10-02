# Persistent vectors in compiled macro data

Refs #14. This is an executing bootstrap prerequisite, not completed M3 or M4
acceptance. The complete pinned `PersistentVector` declaration and its supporting
protocols, indexed-array traversal, iterator and reduction helpers are retained
through the reproducible core import. Every method remains present. Source Error
construction uses the existing typed language error adapter; the one `dotimes`
body uses its ordered loop bootstrap. `EMPTY-NODE` uses the pinned compiler's
emitted `EMPTY_NODE` spelling, checked against complete standalone source forms.
The original forms, hashes, patches and EPL notices remain packaged together.

Ordinary small vector literals and quoted nested vectors construct the actual
canonical source type. Macro results traverse canonical PersistentVector,
VectorNode and SourceArray storage, including multiple trie levels and persistent
tails, under the existing depth/node/UTF16 transport limits. Unknown types,
invalid counts/shift/tail/node storage and unsupported metadata fail explicitly.
The bridge retains canonical roots across redefinition and collection; raw arrays
and trie nodes are never returned as syntax vectors.

The new Array.slice member adapter copies GC-owned source array storage, clamps
relative indices after numeric conversion and truncation, supports optional bounds,
and uses a shared receiver-free function root. Detached calls and unsupported arity
fail. Array property writes and other unimplemented member names retain their
existing diagnostics. This supplies the source vector pop operation without
replacing it with a host vector algorithm.

Seven native tests execute actual artifacts: persistent tail update/association/pop;
a macro returning a binding vector and executing its expansion; a 1,100-element
vector crossing a trie-depth boundary with old values preserved through updates
and GC; nested quoted symbols/vectors and malformed nominal data; and slice bounds,
copy independence, shared method identity and detached-call failure; retained transient construction of quoted vectors across 31/32/33/65/1,057 entry boundaries, original-value preservation and post-persistent errors; and 40 fresh pinned scalar observations independently decoded in both Runtime and Macro Stores. The larger
trie stress explicitly uses a bounded 100-million operation fuel allowance because
the default 10-million allowance was exhausted, not because it was a semantic pass.

The complete pinned transient helper/type declarations, array-copy and public
transient/persistent!/conj! wrappers now execute the retained fromArray factory.
The factory preserves no-clone behavior below 32 entries and the source transient
transition for larger arrays. Persistent decoding clears only the root edit-token
requirement: descendant nodes may retain old tokens, exactly as the pinned
persistent! algorithm permits. An active root token still fails transport.

Current limitations remain explicit: vector/vec wrappers, chunked sequences,
printing, pending integer and modulus helpers, complete metadata maps and public
vector/core acceptance are unfinished. The bridge continues to reject nonnil
metadata. Maps/sets, &env, syntaxquote/unquote/splicing/gensyms, versioned
bootstrap/cache keys, privacy/AOT integration, evaluator removal and #15 stackless
cancellation/pending-I/O/live-GC accounting remain required by the accepted design.
This executing prerequisite is not complete compiled bootstrap or M4 acceptance.

Focused tests:

```sh
CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test -p suss-cli --test compiled_macro_vectors --test compiled_macro_forms --test compiled_macro_indexed_data --test compiled_source_macros --test portable_arrays --test portable_vector_trie --locked -- --test-threads=2
```

The first regression failed with unresolved Macro PersistentVector. Subsequent
checks exposed the unadapted Error construction, munged property name, missing
ILookup, absent Array.slice member and bounded stress fuel exhaustion; all are
recorded in the handoff. No failures were hidden by skips. Fresh pinned compiler/Node execution produced 40 exact scalar observations,
and independently decoded execution matches all 40 in each of both native phases.
Run `sh scripts/test-macro-vector-oracle.sh` to reproduce the reference and native
checks; JVM/Node remain development-only oracles. Full baseline, independent PR
review and final-head CI remain required before readiness. The first full baseline
stopped on a stale test assuming the vector class was absent. That test now uses a
fresh session with no imported core, preserving the original compile-atomic
missing-constructor guard, while real imported constructors have separate tests.
