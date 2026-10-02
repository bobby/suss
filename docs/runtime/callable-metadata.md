# Ordinary callable values and function metadata

This M3 prerequisite lowers ordinary source calls through the existing HIR, explicit
control-flow IR and shared ABI. Evaluate the callee once before every argument.
Canonical closures keep universal invocation; other values use the canonical phase
IFn key for their receiver-inclusive source signature. A checked adapter captures
the actual nominal method before arguments execute, then invokes it with the physical
receiver and argument array. The retained MetaFn, Keyword, Symbol, vector and map
methods own behavior. Explicit source `-invoke` retains its distinct pinned receiver
convention and native protocol fallback.

Function-literal metadata uses ordinary lexical expression analysis and the actual
retained `with-meta` source. Captured old functions, metadata copies and receivers
stay rooted across fragments and GC. Existing function calls avoid reading IFn
bindings through a lazy branch. The new private `callable-bind` runtime export adds
no recursive GC types, layouts or ABI-version change.

Provenance: existing complete core.cljs IFn/MetaFn/with-meta declarations remain
licensed and unchanged at pin c4295f303100bbf5afac449242d30bca1126f1a1. Original
Rust adaptation follows cljs/core.cljc adapt-ifn-params/add-ifn-methods and
cljs/compiler.cljc :invoke's callee-method capture. Source-order metadata variance
remains documented in compiled-macro-metadata.md.

## Evidence

Before94209: all three initial callable regressions fail. Focus5261 also failed
validation because the new predicate converted its Boolean twice; the shared
nominal emitter already converts raw i32 results. Corrected focus65606 passes
three callable and four existing identity tests. Fresh oracle38248 initially failed
pinned compilation because an IFn extend-type probe needed grouped signatures;
corrected44107 reached exact comparison and exposed an incorrect initial expectation
for vector extra arguments. Fresh78447 verifies all67 primary observations after
retaining the observed pin result; ten native metadata tests execute66 matching
cases plus one explicit strict-arity contract boundary in EACH Store after GC.
Original56 primary cases remain unchanged. See macro-metadata-cases.json.

The pin ignores extra arguments in the vector probe and returns1. The accepted
portable design requires wrong-arity errors; Suss evaluates all argument effects
first, throws a language error and its catch returns111. This is separately asserted,
not counted as a differential match or skipped. A direct native regression also
asserts the exception, effects and recovery after GC.

Final focused71765 passes18 tests (callable4/identity4/metadata10). ABI98764 first
failed because the fixture tried to install nil as a method, which the runtime
correctly rejects. Use a valid replacement of a different arity; corrected92753
passes46 ABI tests, including captured methods across GC, malformed keys/copied
environments, typed exception tags and recovery.

Full workspace baseline7750 completed successfully on reviewed source5367d075:
931 passed, zero failed and17 existing ignores across88 groups, through final reader
doctests. Independent PR136 review approved that exact source with no significant
findings. Final publication changes documentation only; successful exact-final-head
CI and independent confirmation remain required before readiness. No merge.

## Remaining M3 acceptance

Source apply and complete higher applied-function behavior, HAMT/transients,
reader-derived locations, &env, syntax quote/splicing/deterministic gensyms,
reproducible versioned Java-free bootstrap, complete cache invalidation and legacy
evaluator removal remain required. Stackless scheduling, pending-I/O cancellation
and live-GC counters are also open. No full M3 or complete core compatibility claim.
