# Mutable array foundation: reference evidence

Source array support is not implemented in the portable compiler yet. This is the
next prerequisite for the pinned IndexedSeq, list, variadic rest and collection
implementations, rather than a claim that internal invocation Args arrays already
provide a public array contract. Persistent collection acceptance remains open.

The pinned ClojureScript reference is c4295f303100bbf5afac449242d30bca1126f1a1.
Relevant runtime declarations are core.cljs452 make-array,468 aclone,477 array,
538 aget,545 aset and553 alength. Their corresponding macro expansions are
core.cljc1043 aget,1056 aset,2635 array,2644 make-array and2748 alength.
No source forms have been copied or adapted into shipped code by this preparation.
Upstream source and notices remain in the pinned development submodule.

Forty scalar observations in tests/oracle/array-cases.json match a fresh pinned
compiler/Node execution exactly. They include empty and mixed arrays, object
identity, missing slots, negative/fractional missing keys, mutation return/aliasing,
growth and undefined holes, shallow clone isolation, nested indexing/dimensions,
first-class functions, array native protocol membership/dispatch, ordered effects
and function values retained inside arrays. Expected values use independent
binary64/Boolean tags; they do not depend on a Suss printer or equality function.

The make-array macro has an observable special case: numeric literal sizes expand
to nil-filled array literals. A dynamic size, or a first-class runtime make-array
call, produces undefined holes instead. undefined? distinguishes the results.
Keep these macro and runtime paths separate when implementing the foundation.
The actual pinned macro bodies, not an assumed JVM make-array contract, establish
this difference. The two-argument macro ignores the type operand in its expansion;
a first-class call still evaluates all arguments before invoking the function.

Reproduce primary generation and comparison followed by the pending Suss checks:

```sh
CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target sh scripts/test-array-oracle.sh
```

The primary stage currently passes; all six focused Suss regression groups fail
with located unresolved array/alength names. These are missing-feature failures,
not compatibility successes. No failure is skipped or represented as nil.
The same tests force GC after every observation once execution is implemented.

Next implement mutable, GC-owned source array storage with stable identity through
growth, distinct shallow clones and unchanged callback argument/environment
ownership. Preserve once-only operands, complete nested indexing, native array
classification and the literal/dynamic allocation difference. Register source
provenance and reviews before publishing the implementation. Public named property
coercions, checked-array compiler options, array source literals and full sequence/
collection integration require separate evidence; the current corpus does not
certify those surfaces or complete any issue/milestone.
