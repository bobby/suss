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


## M4 root-collapse acceptance extension

The shared original fixture `tests/oracle/vector-boundary-fixture.sus` exercises
public 1057→1056→1057 pop/association/conj transitions with metadata. The pinned
corpus now has 91 observations: the original 40 remain unchanged and 51 cover
counts, selected boundary elements, metadata, equality and equal hashes. Fresh
pinned Node observations match all 91 exactly; this is reference verification,
not yet a native pass.

The new native test independently decodes every element in both Runtime and Macro
Stores after GC, including the retained original handle. It checks trie shifts
10→5→10, unchanged leaf sharing, copied modified leaf/tail and root reuse on
regrowth. Private field probes stay out of the shared oracle fixture. The original
vector also remains globally rooted; the test does not claim sole-handle rooting.
Independent review found no material defect. The sharing test passed in PR #223
CI; the scalar corpus exhausted its fuel budget. The measured-budget focused run,
required full baseline and final-head CI remain pending. No M4-02 completion is claimed.


### PR #223 measured scalar-corpus fuel budget

PR #223 CI reports seven passes and one failure: the new 1057-element sharing
test passed, while `boundary-roundtrip-equality-forward` exhausted the corpus's
existing 100-million per-operation allowance. A local focused reproduction also
failed: exit 101, zero passes, one failure, seven filtered, 6.65s. These failures
are retained in `/private/tmp/suss-m4-vector-ci-failure.log` and
`/private/tmp/suss-m4-vector-budget-100m.log`.

After the import baseline ended, an isolated diagnostic used a finite 1-billion
ceiling and inspected remaining Wasmtime fuel after each whole eval. It passed
one diagnostic test in 20.28s with no failures/ignores/filters: 14 phase/size
groups, four observations each. Runtime and Macro costs matched exactly.
Identity equality used 767 fuel; independently constructed equal vectors and
shared pop/conj roundtrips had the same traversal costs:

| Vector length | Whole-eval equality fuel |
| --- | ---: |
| 32 | 7,376,354 |
| 64 | 14,879,852 |
| 1057 | 248,329,586 |

Additional lengths 33, 65, 1024 and 1025 were measured. The sampled growth is
consistent with linear traversal rather than quadratic scaling; this does not
prove general complexity or default-budget performance. At 1057 elements, the
comparison of both vectors' hashes cost 23,196,667 fuel. Measurements cover post-reset Wasm evaluation and the small inspection checkpoint overhead. Preparation resets fuel before evaluation; expansion/compiler cost was not measured. The diagnostic checked actual Boolean results
and ran GC between observations; it is cost evidence, not full corpus acceptance.

The corpus alone now uses a finite 500-million per-operation allowance, about
2.01 times the measured maximum. Existing sharing/smaller stress limits and
production/default budgets remain unchanged. All 91 cases, expectations,
independent decoding and GC checks remain intact, with no skip or retry.
Measurement log: `/private/tmp/suss-m4-vector-fuel-measurements.log`; archived
source: `/private/tmp/suss-m4-vector-fuel-diagnostic.rs`. The temporary test was
removed before the coordinator's full focused run.

The complete `cargo test -p suss-cli --test compiled_macro_vectors --locked -- --test-threads=2` run passed all eight tests, with zero failures, ignores or filters (18.42s, exit 0). Log: `/private/tmp/suss-m4-vector-focused-500m.log`. The required full baseline and repaired final-head CI remain pending; the earlier 100M failures are retained.
