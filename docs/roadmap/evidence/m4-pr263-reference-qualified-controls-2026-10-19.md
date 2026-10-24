# Reference-qualified metadata controls and current native feedback

Source under test: `1bea05084ee38f4e442927bff4fa57d052a40c7d`.
[CI39299666250](https://github.com/bobby/suss/actions/runs/39299666250).
The accompanying archive `m4-pr263-reference-qualified-controls-2026-10-19.tar.gz` has SHA256 `5ce7af381615339083e77050b37eaea419665c1f53a2ba9842ef8567a479111f` and 40 entries.
Its raw logs and receipts preserve failed and unexecuted outcomes separately.

## Actual native execution on 1bea

Foundation job118221379583 completes normal bootstrap reproduction and identity,
compiled_bootstrap4PASS/0FAIL/0ignored (56.50s), CLI library36PASS/0FAIL,
and compiler library466PASS/5FAIL. The three previous AST observer failures pass,
as do both shared-map absent-remove controls and the new provenance/current-owner
and typed-fuel/IRQ type-value controls. The failed foundation supplies no completed
workspace or doctest verdict.

Five compiler failures remain: strict source-child Wasm fuel exhaustion (current
phase/variant not printed); two retained-reader/declaration identity failures;
expected primitive wrapper spans; and outer scalar tag metadata in the ordinary
projection comparison. For that last comparison, actuals[0] is legacy and lacks
outer-tag source metadata; actuals[1] is new and preserves it. Both inner metadata
maps preserve the located tag. The direction correction and original raw log are
retained. Only two of 14 Runtime ordinary controls were reached; subsequent12 and
Macro remain unproved. The type-value test collected 16 Runtime rows, but only its
first deferred comparison ran before failure; Macro comparisons are unexecuted.

Terminal integration shards0/1/3/5 respectively have 173 PASS/13 FAIL,
144 PASS/8 FAIL/3 existing ignored,145 PASS/31 FAIL,130 PASS/10 FAIL. Other shards were live
at this capture; no complete CI total is claimed. Default core-loading native 10M
exhaustion remains, including all 17 core_import controls. Several declaration/self-
local tests stop during core loading, before their semantic assertions. The pinned
map's original 200M control fails during compiled invocation with zero observed
assertion IDs. Historical 69-ID success is not acceptance on this source.

## Actual indexing-reader reference

`node pr261-scalar-tag-indexing-reference-1bea-v1/probe.js` ran in two fresh Node
processes against 115 hash-frozen retained JS/JSON inputs from ClojureScript pin
`c4295f303100bbf5afac449242d30bca1126f1a1`. Both exited 0, stderr empty,
stdout byte-identical: SHA256
`e8301d66d8a82c7f63f8f64c08d9ca2a0cbee774ddec16bfe42397ad47d118e7`.
Each executes 15 actual analyzer-to-registered-&env-macro observations: five source
programs through plain, indexing and filename-indexing readers. These are retained
compiled-reference executions, not fresh CLJS builds or native Suss acceptance.

For `(def ^number subject 17)`, plain reading gives an unlocated tag. Indexing
preserves line 1, column 7, end-line 1, end-column 13 on the raw/final/inner tag, all the
same Symbol object. Filename indexing additionally preserves file. Explicit marker
metadata survives; primitive string tags have no metadata; explicit function tag
and return tag preserve locations. Pinned analyzer.cljc lines 2092–2172/3630–3645 policy
is EPL-1.0; native orchestration is original code. Independent review verifies
both reference runs, source pin, frozen input closure and exact observations.
The proposed production raw-scalar restoration was withdrawn and is not applied:
the new writer agrees with the actual indexing-reader reference.

## Two independently reviewed test corrections

The type-value fixture changes only parsed expected sixteen primitive wrappers'
synthetic spans to decoder 0..0 after asserting primitive-only, metadata-empty
schema. Actual output, semantic vectors, source metadata, original programs,
phase/order/GC/hash behavior, assertions and fuel limits remain unchanged.
Patch 4606c0823b56fbd5d11dd19cc1be80bbc45ce66a6b48279c057b283b6fb10714;
postimage c6aa0643275d1bcad7118208c09291919c03b09d9a34e69818365d15a94283c7.
The historical reviewed manifest is preserved; its original tag-loss wording is
explicitly corrected by the new manifest and raw direction receipt.

The ordinary comparison preserves all 14 original programs and 22 presence/value
fields. Only exact `(def ^number subject 17)` qualifies legacy outer-tag expected
metadata with the independently executed four indexing-reader positions; scalar
kind/data/presence and all other 21 fields remain independently checked. All other
13 programs retain full legacy comparisons. A five-row regression compares plain,
indexing, filename/marker, string and function-tag cases in both phases after GC,
including Symbol identity where the reference proves it. Expected Forms go through
the same quote/read bridge, retaining full metadata and matching synthetic spans.
Patch 78770975eaddf61e9dbc19cca5f4fa8f8db6facaa0aa0199d52e8ef49342f6d2;
postimage 2754564b6a3c454a860d9944a6dd6383f9bce7e6a7f4cd6744806244ca4b93de.
Independent peer review approves only test-source application and bounded replay.

Both patches apply exactly and `git diff --check` passes. No execution of these
corrected tests or production performance improvement is claimed. All production
source, user 10M defaults, Runtime allowances, strict core17/complete one-scope
source-child controls, pinned-map 200M/200M and dedicated exhaustion/IRQ controls
remain unchanged. Parent 66a55ce's exact workspace run is frozen separately; no
concurrent local Cargo/Suss VM was used. Unintegrated source-controller candidates
remain outside these changes.

Test source affects compiler identity. Next run normal two-phase/two-generation
bootstrap reproduction, verify/adopt the exact source-bound tuple, then execute
both corrected controls and all original literal/type-value/reader/default-core/
source-child/source-tags/pinned-map gates. Focused source corrections do not
substitute for the exact workspace command, independent final review or final-head CI.
