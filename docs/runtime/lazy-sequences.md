# Retained lazy sequence dependency for compiled syntax quote

The pinned syntax quote reader expands collection templates through `concat`,
`seq` and `sequence`. Its deferred sequence type must keep its actual lazy
behavior. The retained core now includes the complete pinned `IPending` protocol
and `LazySeq` declaration; this is a dependency in progress for M3 compiled macros,
not complete syntax quote, lazy core or collection compatibility.

Source provenance is ClojureScript `c4295f303100bbf5afac449242d30bca1126f1a1`,
core.cljs814–819 and3584–3667. The import recipe, review overlay and generated
manifest record the exact declaration/file hashes and license packaging. The
explicit LazySeq patch preserves every method and mutable field. It expands
anonymous-function shorthand to special `fn*`, preserving capture and avoiding
the source field named `fn`. It substitutes the pinned default false `LITE_MODE`
constant and removes its redundant symbol type hint from that boolean literal.
Both source branches remain present. Upstream EPL notices are retained.

Two executing native tests initially failed because the actual core constructor
was absent. The first patch attempt retained a type hint on a scalar and failed
reader metadata validation. The corrected constant allowed realization to pass,
but metadata copying invoked the thunk because the shadowable `fn` macro name
was used. Correct `fn*` expansion repaired that error. A later test-only correction
uses the portable `catch :default` syntax. The two original tests then passed in
both Runtime and Macro caller Stores after GC.

The shared original source fixture executes once-only deferred realization,
repeated access, metadata copying without realization, a thrown thunk and retry.
Fresh pinned ClojureScript artifacts, built with forced compilation and analysis
caching disabled, executed in Node and first produced12 ordered scalar observations, then an expanded fresh build produced27.
The native shared fixture matches all27 after GC in both caller Stores; its original12 observations remain unchanged. All four
native tests pass, including direct `IPending` checks. Affected sequence/metadata
suites pass29 tests across four groups; the handoff records terminal commands.
The primary compiler's two warnings about intentional global replacements remain
in the compile log. No failing or unknown observation is replaced by success.

```sh
sh scripts/test-lazy-sequence-oracle.sh
python3 scripts/core_import.py --check
python3 -m unittest discover -s scripts -p 'test_lazy_sequence_oracle.py'
```

The oracle comparator rejects missing/reordered/duplicate cases, unknown fields,
nonfinite/non-numeric values and changed binary64 bits, including signed zero.
Four verifier tests pass. The subsequent constructor dependency slice expands
provenance to274 generated files/270 selections. The review overlay has371
in-progress reviews/694 unassessed declarations, including private sequence
coercion with its unported public arities explicitly recorded.

Whole pinned `ChunkedCons`, `chunk-cons`, `chunk-first`, `chunk-rest`,
`chunked-seq?` and all-arity `concat` now execute. Explicit defn and lazy-seq
constructor adaptations preserve all deferred, chunked and unchunked branches;
the patch records the upstream lazy-seq macro hash and range. The native concat
regression failed before at unresolved concat, then passed with zero/one/two/
variadic arities, unforced tail effects and31/32 chunk boundaries.
Bounded lazy macro-result transport now realizes canonical LazySeq data through
a captured source sval helper, with one shared fuel budget and the existing
inspection/root/exception cleanup scope. It validates canonical ArrayChunk
cursors for ChunkedCons and preserves outer metadata. Five transport guards cover
executing outputs, malformed metadata before thunk effects, cycles, bad cursors,
throw/recovery and undefined sequence-tail normalization. Syntax quote reader
expansion now has focused executing evidence; this transport is still awaiting publication review
and full final-head validation. Deferred printing,
the complete core dependency closure and M4 collection acceptance remain open.
Full final-head baseline, independent PR review and CI remain required before
publication readiness. JVM/Node are development oracles; shipped source has no
Java dependency. No runtime ABI version change is made by these source types.

The shared fixture now includes23 constructor observations, giving50 total
fresh pinned/native matches with the original27 unchanged. Complete `vec` and
`into-array` source branches and both map constructors' variadic/apply paths
execute in Runtime and Macro Stores. Array aliasing, vector metadata removal,
map-entry conversion, chunk boundaries, duplicate keys, nil keys and18-key maps
have explicit observations. Missing-value messages retain the pending `str_`
dependency. The combined native artifact uses one explicit50million operation
fuel budget; its earlier10million run exhausted fuel and is retained in the log.
Separate constructor tests pass2/0/0 and lazy suite4/0/0.
