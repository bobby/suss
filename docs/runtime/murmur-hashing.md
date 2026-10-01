# Retained Murmur hashing preparation

This is unexecuted preparation on a separate branch, not a compatibility claim.
PR95 establishes checked scalar bitwise/imul and retained bit-count/rotation;
its independent review/full baseline and final-head CI remain required.

Pinned scalar Murmur dependencies are m3-seed/m3-C1/m3-C2, m3-mix-K1,
m3-mix-H1, m3-fmix, m3-hash-int, hash-long and mix-collection-hash. Preserve the
original algorithms using explicit fixed defn bootstrap patches and EPL provenance.
The original ->/as-> syntax requires bounded expansions before those forms execute;
zero? requires strict primitive comparison with numeric zero. Prepared expansion
retains source order, lexical identities, captures, source spans/metadata and tail
context. It has NOT been compiled or tested.35 provisional scalar probes are prepared
with independent mathematical expectations; no primary/native success is recorded.

Do not replace hash-ordered-coll/hash-unordered-coll with argument-array folds.
Their seq/first/next/hash dependencies and real persistent collection types remain
unfinished. String hashing needs exact UTF-16 charCodeAt semantics, numeric hashing
needs the pinned bit reinterpretation rules, and immutable empty-collection constants
need supported source metadata/static publication. Those are separate dependencies.
No source forms are copied, inventory status changed, issues closed or new PR opened
by this preparation. The PR95 reviewer owns the shared local test slot; root has run
only source reads, pure probe generation and diff checks.
