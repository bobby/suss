# Repeated declarations in compiled macro environments

A repeated `declare` previously replaced existing source declaration facts even
when the runtime cell already held a value. Source macros then observed changed
privacy, docs, inferred tags, callable method facts and source positions.

The common definition analyzer now preserves the existing immutable declaration
revision when there is no initializer and merged reader metadata contains a
truthy `:declared`. It validates symbols, namespaces and binding ownership first.
Fresh declarations still create records. Ordinary defs and false/nil `:declared`
replace records; initializer-bearing defs still stage a new provisional record
and execute their initializer. Generated `:declared true` overrides user false/nil metadata, matching the pinned
`vary-meta`/`assoc` expansion. The runtime definition path remains unchanged.

## Primary source and evidence

Policy comes from pinned ClojureScript
`c4295f303100bbf5afac449242d30bca1126f1a1`, `core.cljc`175–176 and
`analyzer.cljc`2092–2172 (SHA-256 `297802c627474434f1ef868e31f5f9913c290a4e80c509a40c704dced95bbf47`). The implementation
is original Rust; the upstream source retains its EPL-1.0 license and notices in
the pinned development submodule. No upstream source is shipped by this change.

The original development probe
`tests/oracle/repeated-declaration-review-probe.clj` analyzes the exact def
expansion of pinned `declare` and asserts all 18 selected fields remain unchanged
for completed scalar/function records and repeated forward declarations. It also
checks direct truthy metadata and false/nil/ordinary/initializer boundaries.
Run from `tests/oracle` with
`CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache clojure -Srepro -M repeated-declaration-review-probe.clj`.
The corrected boundary probe passes. An initial assertion incorrectly expected
an initializer-bearing truthy-declared def to publish its docstring; the primary
instead retains its new provisional record. The regression now asserts absence.

`compiled_macro_repeated_declarations.rs` executes actual `declare` forms and
inspects both namespace snapshots and the live catalog. All 18 fields retain
presence, value and data shape; reader spans are normalized separately from
selected source-position values. Both caller phases collect GC before decoding.
Existing function values, scalar values and once-only initializer effects are
also asserted. Finite helper/caller fuel is 100 million; defaults are unchanged.

Before the compiler fix the native regression failed with completed scalar,
function and forward record differences in both phases. Runtime checks passed.
After the fix the expanded regression and 39 related checks pass. An additional
regression failed before correcting generated declaration metadata precedence.
Independent review, final-head full baseline and CI remain pending at publication
preparation; current results are recorded in the handoff.

This is partial progress for #13/#14. Full portable environments/inference,
source/macro dependency cache invalidation, evaluator retirement and M3 lifecycle
acceptance remain open.
