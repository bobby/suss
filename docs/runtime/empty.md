# Full retained empty prerequisite (Refs #17, #19)

The combined collection lane needed public `empty` for the unchanged Subvec
metadata contract. This slice imports the complete pinned source form rather
than replacing it with a method-only call. It owns only the empty recipe/review/
patch, reproducible source-import output, `portable_empty.rs`, and this note.
Other combined fixtures, native tests and aggregate documentation belong to the
coordinator. The frozen original Subvec branch is untouched; all 45 expected
Subvec observations remain unchanged.

## Provenance and adaptation

`runtime:empty:1887` is public, fixed arity one, core.cljs lines 1887–1898 at
ClojureScript pin `c4295f303100bbf5afac449242d30bca1126f1a1`. Whole source SHA-256:
`c95e203fb200453ff789397cb060686b0d9930d1cded9b577daa60c1c8a8fb76`.
The original bytes are `runtime/core-import/extracted/0319.cljs`; the adaptation
is `docs/compatibility/patches/collection-empty.json`. Existing EPL-1.0 notices,
upstream LICENSE and epl-v10.html packaging are retained by the importer.

The patch preserves the exact documentation, parameter vector's single argument,
`when-not` nil guard, both ordered `implements?` and `satisfies?` branches,
both live `-empty` calls and the final nil branch. It expands only `defn` to
`def`/`fn`, matching existing bootstrap patches, and qualifies the emitted nil
macro test through immutable `suss.bootstrap/nil?`. This preserves pinned macro
semantics when the public nil predicate is redefined; it does not capture a
possibly redefined public function. Existing protocol adapters retain the direct
nominal test and native/default fallback. The new review is `:adapted` and
`:in-progress`, with the exact macro/protocol dependency IDs and native test paths.
Compiled upstream defn compatibility remains a separate obligation.

Source generation appends one selection: 320 selected forms, 324 generated files.
The 319 prior manifest form records are byte-for-byte equal as parsed records,
including every source/extracted/adapted hash and path. Only the generated source
append, manifest identity/files and new extracted form change. No compiler code,
Cargo configuration or bootstrap Wasm artifact is edited by this slice.

## Focused regressions and evidence

`portable_empty.rs` authors three tests, each running both runtime and macro
Sessions with a finite 100M operation allowance. Its original fixture uses
explicit `def`/`fn` where needed and actual retained protocols; no upstream defn
compatibility is inferred from fixture setup.

The first test independently decodes actual nil/Boolean ABI results for 16 shared
reference cases: nil, false and noncollections, vector/list/map/set/Subvec empty
category/count/metadata, unchanged original values, equality/hash and once-only
argument effects. The same literal fixture and JSON case sources were extracted
from the native test for a fresh pinned ClojureScript oracle compile and Node run.
All 16 complete tagged observations match exactly. Artifacts are outside tracked
source: `/private/tmp/empty-primary.py`, `/private/tmp/suss-m4-empty-expected.json`,
`/private/tmp/suss-m4-empty-observations.json`, and compile log
`/private/tmp/suss-m4-empty-primary.log`. Generated oracle code/output is ignored.
The reference checkout pin and source bytes were checked by the importer.

The second authored test distinguishes direct nominal implementation from actual
native fallback via a number extension: `implements?` false, `satisfies?` true,
and an observable `-empty` effect. Both paths preserve live dispatcher
redefinition. Public nil? redefinitions in both directions leave the private
nil guard correct, and a captured original empty function survives public empty
redefinition/GC. These assertions are native-pending, separate from the 16 fresh
primary observations.

The third authored test requires rooted wrong-arity Error descriptor 1 and exact
UTF-16 `Wrong arity`, typed method Error descriptor 7 with exact message,
unchanged false/nil thrown payloads, and actual reference identity after GC for
both method throws and argument throws. Recovery and once-only effect assertions
remain. Unknown layouts fail; no wildcard language-error acceptance or case skip
is used.

Executed: `python3 scripts/core_import.py` and `--check` (324 files),
`python3 scripts/cljs_reviews.py` (421 reviewed/644 unassessed), ten focused Python
review-validator tests, Rustfmt edition 2024 and `git diff --check` passed.
An initial authoring command used the wrong recipe field name, then the case-count
assertion was corrected to the actual 16 cases; no native run resulted. Formatting
was rerun from the repository root after an incorrect oracle-directory invocation.
Those setup failures are not native acceptance evidence.

Independent final read-only review found no material findings in the eight owned
files. It reproduced the 324-file import check, 421/644 overlay validation,
16 saved complete reference comparisons and preservation of the 319 prior form
records. This is static/source evidence, not native acceptance. The scoped review
diff and receipt are `/private/tmp/suss-m4-empty-review.diff` and
`/private/tmp/suss-m4-empty-review.json`.

## Required next gate

The author ran no Cargo; the coordinator regenerated both bootstrap artifacts and executed the native gate. Coordinator focused execution now passes all three native tests in both phases; the same run passes all three unchanged Subvec tests. Log: `/private/tmp/suss-m4-collection-prerequisites-native.log`.
The source-import/compiler fingerprint has changed: the coordinator must
regenerate both runtime/macro bootstrap artifacts **before** the next native run.
After exclusive Cargo ownership is released, run focused `portable_empty` and the
full unchanged `portable_subvector` corpus, then the other combined focused tests,
required provenance/bootstrap checks, full locked workspace baseline and
final-head CI. This prerequisite does not complete #17, #19 or M4.

The coordinator separately reported HAMT 8, chunk 2, lazy 4 and reduction 3 native
passes; those are not results of this empty slice. The Subvec fixture helper was
repaired from defn to def/fn and its 45 primary expectations still match; two
native tests passed and the public corpus failed on unresolved empty. Keep that
failure and the earlier missing-test-target assembly failure recorded until the
regenerated-bootstrap focused rerun establishes a new result.
