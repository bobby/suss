# Retained hash-cache macro preparation

Unvalidated local preparation: the original bounded HIR expansion follows pinned
core.cljc caching-hash1284. No production/native acceptance is claimed yet.
Twenty-four provisional scalar probes still need fresh primary execution and
independent native decoding. Full List hashing/core and compiled macros remain open.

The intended expansion reads the symbol cache key once, tests nil or internal
undefined, and returns other cached values including false/zero unchanged. On a
miss it evaluates the hash function before the collection, calls once, assigns
the returned value to the original mutable field/global, then returns that value.
A thrown operand/function must leave the cache unchanged. Receiver fields remain
anchored through nested closures and Object methods. Immutable locals/fields and
non-symbol keys must fail with located compile diagnostics and atomic publication.

The strict development harness is scripts/test-caching-hash-oracle.sh and the
provisional shared corpus is tests/oracle/caching-hash-cases.json. Provenance is
clojurescript/src/main/clojure/cljs/core.cljc at
c4295f303100bbf5afac449242d30bca1126f1a1. The macro source is not copied; upstream
inventory retains its source hash/license. Do not strip caching-hash from retained
List/Cons blocks or treat unspecified hashes as successful observations.

Next acquire the exclusive shared test slot after PR93 review, certify the corpus,
run focused guards, update inventory/handoff only with observed results, and
require independent PR review/full baseline/exact final CI before readiness.
