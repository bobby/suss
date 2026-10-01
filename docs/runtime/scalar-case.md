# Scalar case bootstrap for hashing

Source provenance: pinned core.cljc case2405–2474 and private assoc-test2383–2398,
SHA256 recorded in the compatibility review overlay, upstream EPL1.0. Rust
lowering is original; no upstream macro source is copied. This is a bounded
bootstrap dependency for public hash, not complete compiled case/case* acceptance.

The selector is evaluated once into a fresh binding. Grouped scalar constants
short-circuit their comparisons and execute only the selected result. Number and
UTF16 string tables use private primitive identity comparison, as the pin's
case* path does. Tables containing boolean/nil constants use live qualified core
=, constant first, preserving redefinition and ignoring lexical = shadowing.
Results are analyzed in source order and retain tail recur. Runtime aliases,
exclusions and lexical macro shadowing use ordinary phase namespace resolution.

Explicit defaults and scalar literal constants are required by this bootstrap.
Number/string tables are bounded to256 constants; generic scalar equality tables
are bounded to8, before collection-backed macro map iteration is required.
Composite/symbol/keyword/const-var tests, full macro maps, no-default printer
errors and constant-time case* dispatch remain unfinished. The duplicate guard
uses the accepted binary64/UTF16 representation, including signed-zero aliases.
No unsupported form silently becomes a matching value.

Fresh observations:33 exact primary/native value matches, preserving the first31
certified cases, plus1 separate exact reference boundary. The empty grouped
clause `(case 1 () 17 19)` compiles to invalid pinned JavaScript with a return
before a case label. Node rejects it with SyntaxError: Unexpected token 'return'.
The reference harness checks exit1, empty stdout, exact diagnostic and source
caret; a changed failure or unexpected pass fails. Native rejects the same input
with a located nonempty-group diagnostic before publication. This is neither a
skip nor a value match. Its source is retained in the boundary corpus.

Initial regression41004 failed101 on unresolved Runtime case; implementation3264
passed1. Initial fresh88591 ended1 on the empty-group generated-JS parse failure,
/private/tmp/suss-case-primary32.log. Rather than assign a successful value to
that failed reference, it is isolated and asserted with its native diagnostic.
Fresh63374 ended0:31 value matches/1 boundary/native3. Fresh17662 ended0:33 value
matches/1 boundary/native4, /private/tmp/suss-case-primary33-boundary1.log.
Compiler95117 ended0: validated fragments execute and survive GC in Runtime and
Macro phases, /private/tmp/suss-case-both-phases.log. This does not run compiled
upstream macros or complete macro bootstrap. Root full16020 ended0 through final reader doc tests,
/private/tmp/suss-case-full.log. Independent review/final-head CI pending.

Commands use shared target/build2/--locked/--test-threads=2 without RUSTFLAGS:

```
sh scripts/test-case-oracle.sh
cargo test -p suss-compile --test portable_case --locked -- --test-threads=2
python3 -m unittest discover -s scripts -p 'test_*.py'
python3 scripts/core_import.py --check
python3 scripts/cljs_reviews.py
cargo test --workspace --locked -- --test-threads=2
```

Core selection97/artifacts101 remain unchanged; only review hashes regenerate.
Overlay192 partial reviews/873 unassessed. Public hash's Date branch and numeric
storage adapters, ordered/unordered composition, full macro/core/collection and
M2–M9 release gates remain required. No issue or milestone closure from this work.
