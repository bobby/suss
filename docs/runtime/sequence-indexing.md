# Sequence indexing and index-search execution

The isolated preparation retains pinned indexed?1606, neg?3086,
linear-traversal-nth1927, nth1947, -indexOf1610 and -lastIndexOf1627.
Explicit source-hash-bound defn patches preserve original algorithms, name
metadata/docstrings/arities and EPL packaging. Checked language errors replace
host Error; in-range string charAt(int n) uses exact UTF-16 numeric indexing;
numeric min/max expansions bind their operands once. Bounded original neg? macro
lowering follows pinned core.cljc1201 less-than-zero comparison. No new runtime
ABI/type/bootstrap-cell or fake indexing implementation.

Import92/artifacts96/reviews185 partial+880 unassessed. Fresh pinned primary
execution certifies56 observations, all independently decoded native Wasm matches;
native2 and Python80 pass. Required full workspace baseline is running. Unsupported-value str_/type/type->str formatting remains explicitly
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

Rebased onto reviewed104 f7e0fca. Next inspect the full baseline and dispatch
independent PR review after opening the indexing PR. Independent PR review/fixes and
exact final reviewed-head CI remain mandatory before readiness; no merges.

Pinned generated core.js confirms indexing helpers inline inc/dec rather than
reading runtime function cells. Original bounded unary arithmetic expansions
follow core.cljc1189/1192, evaluate the operand once and use existing checked
arithmetic. First-class runtime functions remain retained. Three fresh oracle
probes redefine core inc/dec and certify search/nth independence; the original53
certified observations remain unchanged. No upstream macro implementation copied.

Fresh10401 terminal0, /private/tmp/suss-indexing-macro56.log: primary56/native2.
Native99611 terminal output shows2 pass, /private/tmp/suss-indexing-native-final.log.
Python80/import96/setup4/reviews185+880 and diff-check pass. Full75515 running,
/private/tmp/suss-indexing-full.log; no full acceptance claim until terminal.

Full75515 ended101 on two old tests using direct inc/dec calls while asserting
retained function errors. Tests now explicitly capture function values, retaining
all prior effect/error/recovery checks; separate macro arity guards cover inc/dec.
Focused40549 ended0: core_import17/indexing2. Full52953 retry running,
/private/tmp/suss-indexing-full-retry.log. No certified oracle result retargeted.

Required full52953 ended0; inspected /private/tmp/suss-indexing-full-retry.log
through final reader doc tests. Commands: CARGO_TARGET_DIR=/Users/bobby/code/
github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked
-- --test-threads=2. All required enabled suites pass, including indexing2/56,
core_import17 and reviewed parent corpora. Existing manual ignores and explicit
diagnostic9passes/7knownfailures remain unchanged. PR105 draft candidate26a7443
opened with independent reviewer review_pr105 dispatched; exact reviewed-head
CI remains mandatory. All root heavy handles terminal; release slot to reviewer
after evidence push. No merges or issue/milestone closures. Next verify reviewer
findings/CI then retained hashing/printing and remaining M2–M9 acceptance.
