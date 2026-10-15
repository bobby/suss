# Retained hash-cache macro preparation

The original bounded HIR expansion follows pinned core.cljc caching-hash1284.
Thirty-eight fresh pinned observations match independently decoded validated Wasm
under the accepted canonical arithmetic NaN sign rule
with forced GC; four native tests cover the corpus and located compile-atomic
recovery guards. Full List hashing/core and compiled macros remain open.

The expansion reads the symbol cache key once, tests nil or internal
undefined, and returns other cached values including false/zero unchanged. On a
miss it evaluates the hash function before the collection, calls once, assigns
the returned value to the original mutable field/global, then returns that value.
A thrown operand/function must leave the cache unchanged. Receiver fields remain
anchored through nested closures and Object methods. Immutable locals/fields and
non-symbol keys must fail with located compile diagnostics and atomic publication.

The strict development harness is scripts/test-caching-hash-oracle.sh and the
shared corpus is tests/oracle/caching-hash-cases.json. Provenance is
clojurescript/src/main/clojure/cljs/core.cljc at
c4295f303100bbf5afac449242d30bca1126f1a1. The macro source is not copied; upstream
inventory retains its source hash/license. Do not strip caching-hash from retained
List/Cons blocks or treat unspecified hashes as successful observations.

Independent PR #94 review adds 14 probes while preserving the original 24 unchanged:
callee throws suppress collection effects, operand writes are overwritten by the
computed miss result, false results are retained and undefined results recompute,
dynamic bindings and redefs restore the outer cache, callable identity survives
hits, lexical macro names shadow calls, and qualified global keys ignore local
and method-field shadows. Alias/exclusion and both-phase checks are separate;
malformed diagnostics have nonempty source spans and atomic recovery.

Fresh review graph 86874 ended with status 0: all 38 exact primary/native observations pass.
Log /private/tmp/suss-pr94-review-primary-field-fixed.log. Command:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-caching-hash-oracle.sh`.
The provisional reviewer hyphenated-field probe passed the pin but native rejected
its documented unmunged-schema boundary (graph 85454 ended with status 101); that new probe now uses
an unmunged underscore field. Original 24 source/expectations remain unchanged.

One partial macro review brings the overlay to 83 reviewed / 982 unassessed; no source macro is copied
and 28 retained selections / 32 licensed artifacts change only their review hash.
Full List hashing and persistent collections remain open. Root must require the
exact final reviewed-head CI before readiness; no merge or issue closure.

Candidate CI run 36813807935 exposed an architecture-dependent arithmetic NaN
sign: Linux produced fff8000000000000 where ARM produced 7ff8000000000000.
The accepted design's 2026-09-29 clarification already permits exactly this
canonical arithmetic sign difference. Only nan-is-cached applies that comparison;
its raw decoded bits and original positive reference expectation remain intact.
Other values still compare exactly. Payload changes, infinities, finite values,
signed zero, and sign differences in other cases are rejected. A separate native
regression produces both signs and checks exact before/after cache-hit and
post-GC storage bits, with throwing operands certifying hit suppression. No
arithmetic canonicalization or runtime change is introduced.

Final fresh graph 67526 ended 0: 38 pinned observations and all four native tests
pass. Log /private/tmp/suss-pr94-review-primary-nan-fixed2.log. Required full
workspace graph 19936 ended 0, including all four cache tests and ABI20; existing
manual ignores and 9 passing / 7 failing diagnostic observations stay explicit.
Log /private/tmp/suss-pr94-review-full-baseline-nan-fixed.log. Python 71 and all
provenance checks pass (graph 93228). Exact final-head CI remains the readiness gate.

Callable hash functions use the same callee preparation as ordinary source calls.
On a miss, a provided IFn object's callable method is prepared before the collection
operand executes. A hit still evaluates neither operand. The supplemental shared
fixture [caching-hash-callable-case.json](../../tests/oracle/caching-hash-callable-case.json)
checks an ordinary callable-object control, a cache miss, a hit with throwing
operands, and the invocation count. Its independently tagged result is
`[71 71 71 1]`; the retained
[pinned observation](../../tests/oracle/caching-hash-callable-reference.json)
records the exact binary64 values. Native coverage decodes the returned vector
after GC in Runtime and Macro sessions. Run
`scripts/test-caching-hash-callable-oracle.sh` for a fresh pinned compiler/Node
comparison and the focused native regression. The original 38-case corpus is
unchanged.

This is an original compiler adaptation of the ordinary call in pinned
`core.cljc` lines1284–1292, under that source's EPL1.0 provenance. No upstream macro
body is copied, no shipped JVM dependency is added, and the existing adapted macro
inventory entry retains its source hash and partial classification. Its linked
native test and this adaptation document include the supplemental evidence.
