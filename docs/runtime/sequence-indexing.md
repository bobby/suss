# Sequence indexing and index-search preparation

The isolated preparation retains pinned indexed?1606, neg?3086,
linear-traversal-nth1927, nth1947, -indexOf1610 and -lastIndexOf1627.
Explicit source-hash-bound defn patches preserve original algorithms, name
metadata/docstrings/arities and EPL packaging. Checked language errors replace
host Error; in-range string charAt(int n) uses exact UTF-16 numeric indexing;
numeric min/max expansions bind their operands once. Bounded original neg? macro
lowering follows pinned core.cljc1201 less-than-zero comparison. No new runtime
ABI/type/bootstrap-cell or fake indexing implementation.

Import92/artifacts96/reviews183 partial+882 unassessed and new53 candidates are
UNVERIFIED preparation. Fresh primary/native execution and focused/full validation
remain pending. Unsupported-value str_/type/type->str formatting remains explicitly
uninitialized; other persistent types, full compiled macros and surrounding
collection/release gates remain unfinished. No completed issue/milestone claim.

Commands use shared target/build2, --locked/--test-threads=2, no RUSTFLAGS:

```
sh scripts/test-indexing-oracle.sh
cargo test -p suss-cli --test portable_indexing --locked -- --test-threads=2
python3 scripts/core_import.py --check
python3 scripts/sequence_provenance.py
python3 scripts/cljs_reviews.py
cargo test --workspace --locked -- --test-threads=2
```

Next rebase onto reviewed104, execute the retained artifact and correct actual
failures with source/order/provenance aligned. Independent PR review/fixes and
exact final reviewed-head CI remain mandatory before readiness; no merges.
