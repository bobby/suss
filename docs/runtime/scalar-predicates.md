# Scalar predicates

`nil?`, `false?`, `true?`, `number?`, `string?` and `identical?` are
first-class functions in `suss.core`, with `cljs.core` aliasing the same cells.
Calls check arity and evaluate operands once, in source order; rebinding a
predicate changes later calls but not captures of the original.

- `nil?` is true of nil. `false?` and `true?` accept only `false` and `true`,
  so neither 0 nor the empty string is a boolean.
- `number?` is true of longs and doubles, NaN and the infinities included
  ([ADR-0003](../adr/0003-clojure-numeric-tower.md)).
- `identical?` compares numbers by value and strings by their UTF-16 code
  units, as ClojureScript's `===` does; other values compare by reference. A
  long and a double are never identical; `0.0` and `-0.0` are, and `##NaN` is
  not identical to itself.

`identical?` differs from JVM Clojure for numbers and strings, where it compares
boxes by reference: `(identical? 1000000000000 1000000000000)` and
`(identical? 1.5 1.5)` are false there, true in Suss, and the JVM's answer for
small longs depends on its `Long` cache. Code that needs value equality uses
`=` or `==`.

The compiler-owned `suss.bootstrap/number?` and strict identity serve core
itself and the WIT adapter ([WIT numbers](wit-numbers.md)), so redefining the
public predicates can't change core's equality or the boundary's checks.

Tests: `crates/suss-cli/tests/portable_scalar_predicates.rs`.
