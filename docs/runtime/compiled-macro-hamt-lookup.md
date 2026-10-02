# Compiled transient hash-map lookup

This is partial M3 core/bootstrap progress for issue #14. Complete macro
bootstrap, rich `&env`, source locations, syntax quote, cache invalidation and
session cancellation remain open.

Both retained `TransientHashMap` lookup arities now check the edit token before
traversing nodes or returning the nil-key slot. This enforces design section 4:
transients error after persistence. The pin at
`c4295f303100bbf5afac449242d30bca1126f1a1`, `cljs/core.cljs:8202`, does not guard
these lookup methods. This is an explicit design variance. All existing active
branches and other methods remain in the source adaptation, with EPL provenance
and refreshed manifest hashes.

The omitted not-found argument already reaches the native Object method as ABI2
undefined (sentinel 6). An executed probe established that no new compiler or
runtime adapter was needed. Do not replace it with nil: a missing non-nil key in
a populated hash map is nil? but is not identical to nil; an empty map and an
absent nil key return nil. Persistent lookup and explicit nil defaults preserve
their separate behavior. Twelve fresh pinned observations test these distinctions,
present values, explicit defaults and IFn invocation in both phase Stores after GC.

Regressions inspect the actual undefined sentinel, reject both lookup arities and
IFn calls after persistence, retain a captured lookup closure across GC, verify
argument effects occur once in order before rejection, and recover by reading the
persisted map. No undefined syntax transport is accepted and no ordinary function
arity is weakened. The tests use the existing bounded 100M operation fuel; default
budget performance and complete core acceptance are not established.

Run `scripts/test-hamt-lookup-oracle.sh` for fresh development-only JVM/Node oracle
observations and the independently decoded native regressions. The shipped path
has no Java/JavaScript dependency. Commands, results and initial failed probes are
recorded in `docs/roadmap/handoff.md`; full baseline, independent review and
exact final-head CI are required before PR readiness.
