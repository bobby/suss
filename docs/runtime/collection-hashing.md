# Retained ordered and unordered collection hashing

Complete pinned core.cljs1406–1415 hash-ordered-coll and1420–1430
hash-unordered-coll are extracted with source SHA256 and EPL1 notices. Explicit
fixed-defn patches retain metadata, docstrings, loops, count and every source
call. No runtime hash shortcut is substituted. The existing shared-GC ABI and
retained seq/first/next/hash/Murmur helpers execute both algorithms.

Ordered hashing folds signed32 multiplication by31 and each element's hash;
unordered hashing sums element hashes. Both count every element before final
mixing. Unordered composition is insensitive to input order while traversal
still preserves source effects. Nested sequential hashes, equal List/Cons/
IndexedSeq/RSeq values, UTF16 elements, metadata and cached List/Cons hashes
follow the source protocol methods. This does not certify persistent maps/sets,
map-entry types, collision nodes or the other unfinished collection families.

The complete empty-unordered-hash initializer1432–1433 is also retained. Its
unsupported private attribute is explicitly omitted, matching the existing
empty ordered hash bootstrap limitation. Namespace privacy/runtime Var metadata,
compiled upstream macros and M2–M9 release gates remain unfinished.

Initial33112 terminal101: two focused regressions failed, one unresolved
hash-unordered-coll compile diagnostic and one Language exception from sequential
hashing before the declared ordered helper was initialized. Retained-form import
89014 terminal0: both focused regressions pass. Fresh8437 terminal0:44 exact
pinned/native observations and4 native tests,
/private/tmp/suss-collection-hash-primary44.log. Strengthened native guards71740
terminal0 plus public5/default3/identity4; Python82 pass.

Five additive observations preserve the original44 source/expected objects.
Fresh1484 terminal0:49 exact pinned/native observations and4 native tests,
/private/tmp/suss-collection-hash-primary49.log. Raw f64/Boolean decoding checks
exact scalar values after actual validated Wasm execution; GC is forced between
cases. Provisional values were certified only after fresh pinned observations
and independent native decoding matched. Corpus covers empty/nil, scalar and
nested sequences, UTF16, reverse views, cached/metadata values, duplicates/count,
ordered and unordered element effect traces and the empty initializer. The pin
emits expected private-var and development trace-redefinition warnings.

Native exception guards decode the exact thrown Number17 and the exact ABI2 Error
message Wrong arity; arbitrary opaque exception values cannot satisfy them.
A throwing first hash leaves the cache available for successful retry, restores
redefined helpers and keeps prior effects. Cached values and metadata copies
suppress later helper effects across GC. Wrong arity follows the accepted strict
native function contract; it is a separate guard, not a claimed pinned value match.

Selection101/artifacts105;196 partial reviews/869 unassessed. Source extraction,
patch/provenance5 and diff checks pass. Required root full6189 ended0 through final reader doc tests,
/private/tmp/suss-collection-hash-full.log. Independent review and
exact reviewed-head CI remain required before any PR readiness. No issue closes
from this prerequisite slice.

Commands use shared target/build2, --locked/--test-threads=2, no RUSTFLAGS:

```
sh scripts/test-collection-hash-oracle.sh
cargo test -p suss-cli --test portable_collection_hash --locked -- --test-threads=2
python3 -m unittest discover -s scripts -p 'test_*.py'
python3 scripts/core_import.py --check
python3 scripts/sequence_provenance.py
cargo test --workspace --locked -- --test-threads=2
```

Next finish review/CI, then actual persistent vectors/maps/sets and map entries,
remaining sequence/collection semantics and compiled core/macro release acceptance.
Date/Inst and other outstanding portable core work remain required. Issue98 still
defers hashing algorithm evaluation; this work preserves the pinned algorithm.


Independent PR112 review preserved all49 original source/reference objects and
all parent corpora. Six additive probes verify count and live final mixing,
signed32 overflow bases, a captured helper observing current element hashing,
and cached Cons metadata. Initial provisional ordered overflow probe asserted
-32 and fresh pin returned false; direct arithmetic inspection established929,
source was corrected and fresh43535 terminal0 certifies55 exact matches:
/private/tmp/suss-pr112-review-oracle55-final.log. Original49 were never altered.
Native10323 terminal0/5 additionally decodes thrown Number17, verifies that a
second element throw prevents its next call and all third-element effects,
restores live helpers and retains Cons cache/metadata across forced GC.
Python82, import105, setup5 and reviews196+869 pass. No production finding.
Independent full27387 terminal0 through final reader doc tests,
/private/tmp/suss-pr112-review-full.log. Required workspace --locked baseline
passes with existing explicit manual ignores retained. Exact reviewed-head CI
still required.
