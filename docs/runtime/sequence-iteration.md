# Retained sequence iteration and reversal

Preparation selects complete pinned IndexedSeqIterator1644 and RSeq1768 type
forms, preserving every upstream Object/protocol method and mutable field. It
also retains reversible?3356 and rseq3361 with explicit source-hash-bound defn
patches preserving metadata/docstrings/arity and dispatch algorithms. Original
extracted source, patch hashes and EPL notices/packaging reproduce from the pin.
Import86/artifacts90 and176 partial/889 unassessed are preparation counts only.

IndexedSeqIterator reads the live underlying array/string with a mutable offset.
RSeq carries the indexed collection, reverse offset and metadata; its retained
methods depend on source sequential equality, reduction and live indexed access.
Pending public hashing, printing and index search remain uninitialized; invoking
those methods does not silently succeed. Full compiled macros, vector/sorted
collections, generic reverse and host ES6 iteration remain unfinished.

The new53-case corpus is UNVERIFIED preparation. It is intended to execute native
iteration, UTF-16 units, shared/live storage, offset/exhaustion, RSeq reverse order,
metadata/tails/cloning, equality, reduction and early termination. It is not
certified primary/native evidence until actual fresh pinned and Wasm runs pass.
Required focused/full tests, independent PR review/fixes and exact reviewed-head
CI remain pending. No issue or milestone acceptance claim.

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

Next validate/execute these retained methods, then close public/ordered hashing,
printing/index search and remaining persistent collection/compiled macro gates.
