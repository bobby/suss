# Retained sequence iteration and reversal

The canonical import selects complete pinned IndexedSeqIterator1644 and RSeq1768
type forms, preserving every upstream Object/protocol method and mutable field.
It also retains reversible?3356 and rseq3361 with explicit source-hash-bound defn
patches preserving name metadata/docstrings/arity and dispatch algorithms. Original
extracted source, patch hashes and EPL notices/packaging reproduce from
c4295f303100bbf5afac449242d30bca1126f1a1. Import86/artifacts90 and176 partial/
889 unassessed remain bounded evidence, not compatibility completion.

IndexedSeqIterator reads live underlying array/string values with a mutable offset.
It checks current length on hasNext, advances after next and supplies exact UTF-16
units. Separate iterator objects have independent offsets. RSeq carries the
indexed collection, fixed reverse offset and metadata; its retained methods now
execute source equality and reduction over live indexed values. It shares storage,
clears tail metadata as upstream requires, retains metadata on cloning and stops
reduction on Reduced. rseq preserves direct protocol dispatch and returns nil for
an empty IndexedSeq. RSeq itself is not IReversible. No runtime intrinsic algorithm,
shared ABI/type/bootstrap-cell change or fake dependency is introduced.

The shared53-case corpus matches fresh pinned ClojureScript/Node and independently
decoded validated native Wasm, with GC after every case. It covers actual iterator
methods, offset/exhaustion, array growth/current values, UTF-16/nil/false values,
independent offsets, Object-method operand effects, direct iterator reduction,
RSeq fields/order/count/metadata/tails/cloning, cross-sequence nested equality,
source reduction/early termination, live indexed storage and captured reverse count.
Two native tests additionally retain objects across fragments/GC and check typed
errors, unchanged array storage and recovery before subsequent growth/read calls.
Python80/import90/setup4/reviews176+889 checks pass. The required full workspace baseline also passes. Independent PR review/fixes
and exact final-head CI still gate readiness.

## Explicit named-property boundary

The accepted portable contract does not promise general JS/Closure interoperability.
The existing named-property adapter supports array length reads and explicitly
rejects named array writes. Source IndexedSeqIterator/RSeq only read length; this
port does not require adding JS-style length assignment. Numeric aset growth and
live values execute in the shared corpus.

The original `iterator-array-shrink` probe is retained byte-for-byte as a separate
one-case corpus, tests/oracle/iteration-length-boundary.json. Fresh pinned execution
certifies false after assignment; native execution must produce SessionError::Language,
leave retained storage intact and recover after GC. This is an explicit boundary,
not one more matching native value or a skipped success. Both primary corpora run
in scripts/test-iteration-oracle.sh; the native negative test reads the exact
boundary fixture. General named array length writes remain unsupported on this
adapter; no portable core declaration is marked excluded or implemented by this.

Initial fresh47858 ended101: all53 original primary values matched, then native
failed at the length-write boundary. Moved that exact probe to the separate explicit
boundary catalog; the other52 source/expected objects remain unchanged. Fresh8957
certified52 shared and1 boundary primary values, then failed one new native guard
that wrongly expected Object single-fixed-method extras to raise arity error.
Existing pinned Object semantics intentionally evaluate and ignore extras. Removed
that provisional error expectation and added an exact primary/native operand-effect
probe instead. Final26590 ended0 with53 shared matches,1 separate primary boundary
match and native2. Logs preserve both failures; no certified expected value changed.

Pending public/ordered hashing, printing and index-search helpers remain
uninitialized; invoking those methods does not silently succeed. Generic reverse,
vector/sorted collection reversal, complete lazy/chunked/transducer behavior,
compiled macros and host ES6 iteration remain unfinished. This port does not
certify those methods or close an issue/milestone.

Commands use shared target/build2, --locked/--test-threads=2, no RUSTFLAGS:

```
sh scripts/test-iteration-oracle.sh
cargo test -p suss-cli --test portable_iteration --locked -- --test-threads=2
python3 -m unittest discover -s scripts -p 'test_*.py'
python3 scripts/core_import.py --check
python3 scripts/sequence_provenance.py
python3 scripts/cljs_reviews.py
cargo test --workspace --locked -- --test-threads=2
```

Next close public/ordered hashing and printing/index-search dependencies while
implementing remaining persistent types and compiled macro/release acceptance.
