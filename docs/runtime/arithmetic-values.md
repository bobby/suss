# Arithmetic

`+`, `-`, `*`, `/`, `inc`, `dec`, `quot`, `rem` and `mod` follow Clojure 1.12
for longs and doubles ([ADR-0003](../adr/0003-clojure-numeric-tower.md)). The
operations live in the shared runtime (`runtime_abi/longs.rs`); the compiler
inlines them where it can, and they are also first-class functions in
`suss.core`, with `cljs.core` aliasing the same cells. Inside a function, the
compiler keeps longs and doubles in raw Wasm locals and boxes them only where
they escape ([unboxed locals](unboxed-locals.md)).

- Two longs give a long, checked: a result outside the 64-bit range throws
  "long overflow", as `(+ Long/MAX_VALUE 1)`, `(- Long/MIN_VALUE)` and
  `(inc Long/MAX_VALUE)` do in Clojure. `unchecked-inc` and the other
  `unchecked-` operations wrap.
- A double operand makes an arithmetic result a double. Division also follows
  Clojure's source overload: a literal, inferred primitive or hinted double
  selects IEEE division, so `(/ 1.0 0)` is `##Inf`. An unhinted generic helper
  `(fn [x y] (/ x y))` checks zero even when its actual operands are doubles.
  Generic division propagates a NaN operand before checking the divisor.
  Mixed static/Object calls such as `(/ 1.0 (identity 0.0))` retain IEEE results.
  These source facts are independent of whether the backend boxes an operand.
- `/` of two longs that divide exactly gives a long; `(/ 1 0)` throws "Divide
  by zero". A long quotient that doesn't divide exactly is a Ratio in Clojure;
  until Ratio arrives (#338) Suss gives a double, so `(/ 1 2)` is 0.5 and
  `(/ 4)` 0.25 where Clojure gives `1/2` and `1/4`.
- `quot` and `rem` truncate toward zero and `mod` takes the divisor's sign, so
  `(rem -7 2)` is -1 and `(mod -7 2)` is 1.
- With no arguments `+` gives 0 and `*` gives 1; unary `-` negates and unary `/`
  takes the generic reciprocal, including its zero check. Inlined variadic
  division is a nested left fold: a division error prevents evaluation of a
  later operand. Both operands of each binary operation evaluate once in order.
- As first-class values the functions check their arity, evaluate their
  arguments once in source order, and keep their behaviour in captures after
  `suss.core` is rebound.

The auto-promoting `+'`, `-'`, `*'`, `inc'` and `dec'` need BigInt (#338).

Tests: `crates/suss-cli/tests/clojure_equality_bitwise.rs` and the jank
clojure-test-suite's arithmetic namespaces
([oracle](../../tests/oracle/clojure-test-suite-known-failures.json)).

Source overload evidence is in `results/m12-division-overloads`, with a fresh
JVM1.12.6 oracle and Runtime/Macro/AOT regressions in
`crates/suss-cli/tests/clojure_division_overloads.rs`.
