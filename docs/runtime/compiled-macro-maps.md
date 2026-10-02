# Persistent array-map data for compiled macros

This prerequisite for [M3-03](https://github.com/bobby/suss/issues/14) executes
actual retained `PersistentArrayMap`, `MapEntry` and `PersistentArrayMapSeq` source
in the isolated Macro Store and the Runtime Store. Quoted maps share the compiler's
ordinary literal constructor/factory paths. Keys and values retain textual order,
and the factory is captured before entry effects.

The form bridge captures canonical descriptor identities and owned roots. It reads
array maps as paired reader forms, map entries as vectors, and array-map sequences
as lists of entry vectors. GC, class redefinition, unsupported metadata, malformed
counts/storage and the existing traversal bounds remain explicit boundaries. Raw
arrays and foreign classes cannot become macro syntax through matching field shape.

Thirty complete pinned declarations are added with EPL notices and hash-bound
adaptations. Two `#js` arrays become source-array intrinsic calls; known `doseq`,
`doto` and `when-some` sites become ordered lexical loops/lets. The canonical
`HASHMAP_THRESHOLD` property uses the pinned compiler's emitted spelling.
`lookup-sentinel` preserves actual object identity, but private Var metadata is
still omitted by the bounded bootstrap and remains unfinished.

## Executing evidence

`scripts/test-macro-map-oracle.sh` generates fresh observations with pinned
ClojureScript 1.12.134, executes Node, compares exact tagged scalar observations,
and runs the native map tests. Its 24 cases cover empty/1/7/8-entry maps, keyword,
symbol, string and nil keys, duplicate dynamic/equivalent keys, signed zero,
persistent assoc/dissoc, lookup defaults, unordered equality/hash, entries and
key/value reduction. Native observations are decoded independently in each phase,
with forced GC between observations. No constructor fixture supplies these maps.

Focused regressions additionally exercise nested quoted forms, actual entries and
sequences, malformed/foreign storage, old values after class redefinition, factory
capture, entry order and exceptions. See `crates/suss-cli/tests/compiled_macro_maps.rs`.

Independent PR #133 review found that sequence transport counted a generated entry
vector's children at the vector's own depth. The focused before-fix test failed:
62 enclosing lists incorrectly passed the 64-level syntax bound. Children now use
two levels below the sequence, accounting for the entry vector. Regressions in
both stores accept 61 enclosing lists, reject 62 after GC, reject a flat sequence
exceeding the total 4096-node budget and verify recovery. Additional malformed
index, metadata and oversized backing-array cases remain rejected.

The initial large budget probe exhausted the default operation fuel while
constructing its data; it now explicitly uses bounded 100-million operation fuel.
A subsequent diagnostic assertion was corrected to the guard's actual bounded
pair-storage error. Final focused validation passed all six map and three quoted
identifier tests (`/private/tmp/suss-pr133-final-focused.log`). Independent Python
checks passed 88 tests, with 191 imported files and 12 licensed setup forms verified.

## Remaining acceptance

This does not certify full maps, public core or compiled bootstrap. Large literal
maps and array-map overflow still require retained HAMT/transient algorithms;
ES6 iteration/printing, public string lookup and complete collection methods remain
unfinished. Runtime metadata transport, `&env`, syntax quote/splicing/gensyms,
versioned Java-free bootstrap, complete cache invalidation and removal of the
legacy evaluator remain required by M3. The full workspace baseline, independent
PR review and final reviewed-head CI gate publication readiness.
