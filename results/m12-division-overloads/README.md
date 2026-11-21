# Source numeric division overloads

The baseline is main 50681542c126268da35a44435c5861595455b064, without the fix.
Fresh JVM Clojure 1.12.6 executes all 23 oracle rows successfully. Run:

```
mise exec -- clojure -J-Dclojure.main.report=stderr \
  -Sdeps '{:deps {org.clojure/clojure {:mvn/version "1.12.6"}}}' \
  -M results/m12-division-overloads/oracle.clj
```

The initial oracle incorrectly expected NaN divided by boxed zero to throw.
Its actual failure is retained. Clojure propagates NaN before checking the
boxed divisor; the corrected oracle and test preserve this explicit exception.
Clojure 1.12.6 Numbers.java divide(Object,Object) and its double overloads
are the implementation reference; no source is copied into shipped code.

The baseline command is `cargo +stable test -p suss-cli --locked --test
clojure_division_overloads -- --test-threads=2`, under MemoryMax4G, two Cargo
jobs and the shared native cache. Both targets fail on generic-div 1.0 0.0:
Runtime observes ##Inf instead of the expected catchable Divide by zero error;
an actual AOT component returns a mismatch. The Runtime failure occurs before
the Macro phase and the AOT failure stops after the first exported case. This
is not evidence that the remaining Suss rows ran or that Macro passed/failed.
The tests must execute all rows in both phases and AOT after implementation.

Keep source primitive facts distinct from physical boxed/raw representation.
Literal/hinted doubles retain IEEE zero division, mixed static/boxed operands
select primitive overloads, and unhinted generic calls check boxed zero. The
corpus includes signed zeros, NaN/Infinity, ordinary division and arity controls.
The complete issue acceptance also requires effect/redefinition controls,
existing-test review, full correctness checks and independent review/CI.

## Corrected source and effect controls

`controls-oracle.clj` independently verifies 28 JVM1.12.6 rows. It translates
only Suss's `catch :default` spelling to ArithmeticException in the JVM input.
The added rows cover captured primitive locals, Object-typed try joins,
left-to-right argument effects, an early error stopping a later operand, and
captured versus redefined helper functions. `before-order.log` executes the
frozen first-fix CLI and records an incorrect extra :c effect; its exact source
is baseline b8a341bd plus first-fix.patch. The corrected lowering stops after :b.

`focused.log` passes 41 tests, zero failures or ignores across overloads,
unboxed locals, casts, default macro fuel, primitive parameters and entries.
Every one of the 28 overload rows executes in Runtime, Macro and actual AOT.
The separate import-trace case passes; the runtime helper name changed from
number-divide to checked-divide, with all four calls and effects still asserted.
The older zero-division equal-path fixture now explicitly distinguishes source
Object division from source primitive division, retaining both original inputs.

`focused.json` and focused-source.patch identify the measured dirty source and
executable. The initial check failed because the generator and compiler saw
source edits at different times; its complete log is retained locally as
`target/probe/m12-division-check.log`, not counted as a semantic or passing test.
The first-fix pipeline run remains explicitly incomplete: it was stopped for
the material nary-order correction and updated tests, after the 23-row overload
suite passed and its import-name fixture failed. Do not count it as a full pass.

Primary implementation references are Clojure1.12.6
[Numbers.java](https://github.com/clojure/clojure/blob/clojure-1.12.6/src/jvm/clojure/lang/Numbers.java)
and [core.clj](https://github.com/clojure/clojure/blob/clojure-1.12.6/src/clj/clojure/core.clj).
This is original Suss lowering of observed numeric semantics; no JVM source or
Java dependency is shipped. Full new-head checks and independent review remain
required; the prior known-failure suite baseline is not a compatibility pass.

The vendored suite's reviewed known-failure comparison passes unchanged on
b8be026d (suite-baseline.log); suite-baseline.json identifies the complete local
raw capture. This is one test with ten unrelated harness tests filtered, not
proof that every upstream assertion passes. All361 Python checks and the
vendored source lock also pass. Neither the JVM reference nor the reviewed
known-failure JSON is changed to obtain these results.

## Loop source-kind review correction

Independent review of354cbfd4 found one P2: a loop's primitive initializer was
used to select IEEE division even when an Object-valued recur boxes that local.
The frozen before CLI returns ##Inf instead of the JVM's Divide by zero error;
its executable identity and the actual negative output remain in loop-fix.json
and before-loop.log. A later loop initializer is also reanalyzed when an earlier
local becomes boxed, as the fresh loop-initializers-jvm.log confirms.

The correction annotates source division after HIR analysis. It settles loop
source kinds against recur, then visits initializers, dependent aliases and
captured bodies under those settled facts. Primitive-compatible loops remain
primitive. Presence and rewrite memoization retain sharing; expression IDs stay
alive while shallow copies are made. Source inference and backend representation
remain separate, and all work is metered.

loop-oracle.clj verifies38 JVM1.12.6 rows, all executed in Runtime, Macro and
actual AOT. loop-focused.log passes the same41 focused tests with no failures
or ignores. Java-free two-build reproduction and four bootstrap tests pass,
both actual Binaryen133 optimized-component tests pass, all361 Python checks
and 508-file source/provenance checks pass. The reviewed suite baseline still
compares unchanged on the corrected source; raw capture identity is retained.
These are focused gates, not a terminal full-workspace pass. Re-review and
passing exact final-head CI remain required.
