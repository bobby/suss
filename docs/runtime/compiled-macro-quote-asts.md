# Quoted source AST records

A fresh pinned ClojureScript compile and Node execution match fourteen quote
observations. The native regression matches all fourteen in both caller phases
after GC, including datum identity, metadata and an effect counter that stays
zero. This is focused evidence; independent review, the unfiltered full baseline
and exact final-head CI remain pending. It does not complete M3.

The source graph retains selected `:quote` fields, a literal `:const` child, the
child's original form/value, a shared source environment and `:children [:expr]`.
It uses genuine retained syntax and source analysis facts, without classifying
physical constructor HIR as a source child or resolving/evaluating quoted data.
The datum is shared across the parent form and child's form/value; fresh record
allocations and full source-form validation preserve transport budgets.

Quoted scalar operands now lower as private literals. Ordinary expression
analysis previously attached the datum's source record and hid the enclosing
quote. A focused compiler regression checks the retained quote for nil, false,
number and string in both phases. Separate source-tag checks cover metadata
hints, including nil/false, and the distinction between meaningful metadata and
reader/analyzer bookkeeping.

Meaningful metadata on a quoted symbol gives its constant child a tag present as
nil while the enclosing quote infers `any`. These are distinct retained facts.
The initial, previously unverified corpus expected matching tags; actual fresh
primary observations corrected that one case's parent tag, child tag and tag
agreement. All fourteen cases and field/identity assertions were retained.

Cases cover nil, false, number, string, keyword, unresolved/qualified symbols,
empty/nonempty lists, vectors, maps, sets, a metadata-bearing symbol and quoted
effectful-looking data. Both observation helpers guard sequence access so missing
parent fields fail an assertion instead of throwing in the helper. The guarded
parent regression fails on `(quote nil)`, which reports a scalar constant and no
child; the repaired test passes. Its decoder preserves presence bits and data
kinds rather than treating absent and nil values, or booleans and numbers, alike.

Validation so far:

- `scripts/test-quote-asts-oracle.sh`: forced, cache-disabled pinned compilation,
  all fourteen trace rows and all fourteen executed Node projections match.
- Focused compiler source/tag tests: 2 passed; native quote regression: 1 passed
  in 9.30 seconds, with all cases in both caller phases.
- Both Runtime/Macro bootstrap pairs regenerated; `scripts/verify-bootstrap.sh`
  reproduces both pairs without Java and passes all 4 execution tests.
- Affected native suites: 72 passed across 16 unfiltered integration groups;
  graph bounds/callback/sharing tests: 6 passed (focused selection).
- Python checker suite: 134 passed, including 7 strict corruption regressions;
  generated core import check: 274 files verified.

Source reference: ClojureScript commit
`c4295f303100bbf5afac449242d30bca1126f1a1`, `analyzer.cljc` 2630–2655 and
4444–4465, SHA-256
`297802c627474434f1ef868e31f5f9913c290a4e80c509a40c704dced95bbf47`.
Graph mapping and observation helpers are original code. The source inference
adaptation retains its upstream EPL-1.0 notice and records these rule references;
quoted-data lowering keeps its existing provenance. JVM/Node are development
oracles only. The compiler input change is reflected in both bootstrap pairs.

Metadata-wrapped outer syntax, full field/compound/invocation/children schemas
and constant-expression policy remain unfinished. Evaluator retirement,
dependency/target integration, rooted pending-I/O cancellation and code/live-heap
accounting remain original M3 requirements.
