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
