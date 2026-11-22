# Recursive lexical function groups

This is an original Suss implementation of Clojure letfn/letfn* syntax; no
upstream implementation body is copied. Clojure 1.12.6 is the development-only
behavior oracle. The JVM JAR hashes are retained in the oracle command records;
Clojure's EPL-1.0 dependency is not shipped. Public letfn expands to primitive
bindings and genuine core fn forms; its metered clone preserves declarations,
signatures, metadata and source spans. All names are declared before analyzing
any initializer. Duplicate names follow JVM behavior: the last declaration wins.

A private object array holds the recursive group. Closure constructors capture
its reference for forward/peer uses, and its slots are filled before the body
runs. The body's direct lexical values remain available after initialization.
Regular numeric captures still use the existing raw-field eligibility analysis.
Nested functions and futures propagate group-slot access; a future test actually
suspends, collects and resumes before invoking a peer. No shared ABI layout or
new shipped dependency is introduced. Group and closure objects still allocate.

Source facts retain real letfn bindings and function initializers, separately
from the private runtime array. The binding's provisional and initialized
snapshots share lexical identity but have separate memoization revisions. Both
macro phases check those snapshots and the letfn AST's bindings/body children.
This is a bounded source-data check, not certification of the entire analyzer
schema.

## Executed evidence

- The original published compiler rejects the mutual-recursion fixture with
  Unresolved Runtime name letfn; the identical JVM expression returns the three
  asserted values in oracle.log.
- The initial check exposes two missing feature-walker arms. After their repair
  the check passes. Initial execution then exposes a missing special-form
  registration for letfn*. All original logs and initial source reconstruction
  remain; failed runs are not rewritten as passes.
- The first macro fixture incorrectly used Session::eval to define a macro.
  Its failure is retained in macro-fixture-focused.log. The corrected test uses
  the existing CompiledMacros host and eval_with_macros API.
- The four-target focused command passes22 tests, zero failed/ignored: three
  both-phase/AOT/GC tests, one macro snapshot test,14 async tests including the
  new peer-group await/GC check, and four IR/source/diagnostic checks. That run
  used25 JVM pairs, whose exact execution source is retained separately.
- Three additional JVM hint-boundary assertions extend the matrix to28 cases.
  Both phases and actual AOT pass all28 (84 observations), plus the independent
  retained-group GC test. Hinted fn results remain source Objects at generic
  calls: byte127.1 gives127, and an outer long return can narrow the boxed1.5.
- source-check.py reconstructs16 exact compiler/test inputs frome8d0ed9f.
  run-gates.py is active across48 targets, followed by Java-free bootstrap,
  Binaryen133 and reviewed-suite comparison. Its focused log is named
  broad-focused.log; terminal outcomes accumulate in commands.json. The earlier
  unfiltered workspace command stays active on the separate frozen capture
  snapshot; it is not a full pass for this newer source.

## Boxed reference and measurement

The parent compiler cannot execute letfn, so it is not used as a fictitious
before measurement. boxed-source.patch reconstructs the same letfn feature
with only capture-field planning disabled in the emitter. boxed-inputs.json
records17 exact inputs; the normal source and this development reference remain
separate. run-boxed.py executes the same28-case both-phase/AOT/GC fixture on
that reference. Inspect its terminal result before using it as evidence.

measure.py requires that reference test pass and verifies both source snapshots.
It copies immutable compiler executables, compiles identical benchmark forms,
records their hashes and asserts every result. The group performs real mutual
calls over one numeric capture; the direct arithmetic control stays separate.
The six fresh JVM benchmark assertions cover both functions at10/200K/400K.
The driver measures16 null-GC RSS samples and8 instruction samples plus8 warm
checks. These are allocation/instruction measurements, not quiet walltimes.
No measurement result is inferred from merely starting the driver.

The boxed reference passes all3 execution/GC tests on28 JVM pairs, so both
storage choices execute the same84 observations plus the retained-group GC
control. The identical-source measurement finishes successfully: all16 RSS,
8 perf and8 warm results match their asserted sums. The mutual-call group
estimates1832.73/1831.79 bytes per iteration boxed, versus1814.71/1816.27 raw;
about one16-byte numeric box disappears. Instruction deltas fall17956.96 to
17486.72 per iteration. Direct arithmetic stays below RSS resolution/about18
instructions. Recursive group, closure and source-signature allocations still
dominate the roughly1.8KB group sample; this does not eliminate those objects.
