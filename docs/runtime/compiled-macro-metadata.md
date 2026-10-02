# Metadata on compiled macro forms

This M3-03 prerequisite retains reader metadata as actual compiled persistent-map
data across the native form bridge. Quoted symbols, lists, vectors and maps use
retained `with-meta` and `-assoc` source/protocol bindings. Prefix metadata is
normalized before constructing values: keywords mean `{key true}`, symbols/strings
mean `{:tag value}`, and inner prefixes merge before outer prefixes. Reader-key
comparison ignores spans/metadata, shares list/vector sequential data equality,
and compares map/set data without relying on iteration order. Duplicate map entries compare by their last value, and set members compare
by unique data, including native Forms with repeated entries. An existing inner
key object remains while the outer value wins. A depth64/comparison-work bound
keeps this syntax normalization bounded. Actual compiled association supplies
equality of evaluated runtime keys; the reader-data comparison does not replace
that runtime operation. Ordered HIR temporaries avoid deep association chains.

Ordinary vector/map/set metadata evaluates expressions in the surrounding lexical
environment after collection entries. The `with-meta` callee is captured first.
Reader merging removes overridden expressions before analysis or evaluation.
Quoted literals, empty-list constants and native form transport retain metadata
as data. Discarded data values are not constructed, so redefining a public vector
factory cannot expose discarded metadata. Surviving metadata entries use textual
order, following the accepted source-order decision instead of the pinned reader's
incidental hash iteration or prefix insertion order.

Ordinary runtime constants and collection literals elide the six reader-location
keys and `cljs.analyzer/analyzed`, matching the pinned analyzer/compiler. The
internal `suss.bootstrap/quote-form` compiler form shares quoted-data lowering but
preserves explicit reader metadata for native form transport and `&form`. This
purpose is explicit; Runtime and Macro phases have the same literal semantics.
The form bridge calls it directly on reader forms, without printing or rereading.

The decoder accepts canonical metadata maps, preserves them as one reader metadata
form at the macro call span, and traverses them under the existing depth/node/UTF16
bounds. Nil differs from an explicitly empty metadata map in native form transport.
Non-map or foreign metadata and cycles remain errors. Metadata of each syntax node
is distinct from metadata on the backing objects of its sequence representation.

Seven complete pinned chunk declarations (`IChunk`, `IChunkedSeq`, `IChunkedNext`,
`ArrayChunk`, `array-chunk`, `ChunkedSeq`, `chunked-seq`) are retained for actual vector chunk sequences. Defn bootstrap and typed Error adaptations keep all signatures,
methods and branches, with source hashes and EPL notices. Canonical chunk sequences
are decoded from actual vector/node/trie storage, not an eager fake collection.

The complete pinned `js-mod` wrapper is retained with an original binary64
remainder adapter. Exact power-of-two scaling and subtraction implement the
remainder without a host import. Scalar coercion, signed zero, infinities, NaN,
maximum finite values and subnormals are covered. General object coercion remains
unsupported and produces a language error.

## Evidence and gates

Fresh pinned compiler/Node comparison matches 56 tagged observations, including
actual macro-time `&form` metadata, vector equality, chunk boundaries and numeric
remainder edge cases. Every observation also passes independently in the Runtime
and Macro stores after forced GC. Ten metadata tests cover nested metadata,
prefix precedence, reader-key elision, returned sequence metadata, malformed data,
cycles, actual vector trie traversal, core-callee capture, ordinary lexical/effectful
metadata, reader-key retention, discarded constructor suppression and ordered
remainder arguments. The combined metadata/forms/maps/vectors/identifier focus
passes 32 tests; the ABI and bitwise focus passes 48 tests.

Initial native vector equality failed because the retained RangedIterator needed
`js-mod`; source inspection narrowed the generic language failure to this missing
binding. Retaining its complete wrapper and implementing the remainder resolved
the failure. The original case remains in the corpus. See
`scripts/test-macro-metadata-oracle.sh` and
`crates/suss-cli/tests/compiled_macro_metadata.rs`.

Source tests preserve compile-atomic rejection of invalid metadata. Previously
unsupported valid metadata is now covered by positive tests; no blanket skips or
removed case catalog entries establish success. Full baseline, independent review,
significant fixes and final reviewed-head CI remain PR readiness gates.

## Remaining acceptance

This does not establish full M3 or collection compatibility. HAMT/large metadata
maps, transients, function literal metadata, remaining chunk methods/public APIs,
general object numeric coercion,
ES6/printing, full reader-derived source locations, `&env`, syntax quote/splicing,
deterministic gensyms, reproducible versioned Java-free bootstrap, complete cache
invalidation and legacy evaluator removal remain required. Stackless scheduling,
I/O cancellation and live GC accounting remain separate original M3 obligations.
