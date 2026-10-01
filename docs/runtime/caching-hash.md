# Retained hash-cache macro preparation

The original bounded HIR expansion follows pinned core.cljc caching-hash1284.
Twenty-four fresh pinned observations match independently decoded validated Wasm
with forced GC; two native tests cover the corpus and located compile-atomic
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

Fresh graph46126 ended0: all24 exact reference observations and both native tests
pass. Log /private/tmp/suss-caching-hash-primary-candidate.log. Command:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-caching-hash-oracle.sh`.

One new partial macro review brings the overlay to83/982; no source macro is
copied and28 retained selections/32 licensed artifacts change only their review
hash. Next require independent PR review, significant fixes, the full baseline
and exact final-head CI before readiness. Persistent collections remain open.
