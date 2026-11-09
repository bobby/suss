# Arithmetic

`+`, `-`, `*`, `/`, `inc`, `dec`, `quot`, `rem` and `mod` follow Clojure 1.12
for longs and doubles ([ADR-0003](../adr/0003-clojure-numeric-tower.md)). The
operations live in the shared runtime (`runtime_abi/longs.rs`); the compiler
inlines them where it can, and they are also first-class functions in
`suss.core`, with `cljs.core` aliasing the same cells.

- Two longs give a long, checked: a result outside the 64-bit range throws
  "long overflow", as `(+ Long/MAX_VALUE 1)`, `(- Long/MIN_VALUE)` and
  `(inc Long/MAX_VALUE)` do in Clojure. `unchecked-inc` and the other
  `unchecked-` operations wrap.
- Any double operand makes the result a double, with IEEE-754 behaviour:
  `(/ 1.0 0)` is `##Inf` and `(/ 0.0 0)` is `##NaN`.
- `/` of two longs that divide exactly gives a long; `(/ 1 0)` throws "Divide
  by zero". A long quotient that doesn't divide exactly is a Ratio in Clojure;
  until Ratio arrives (#339) Suss gives a double, so `(/ 1 2)` is 0.5 and
  `(/ 4)` 0.25 where Clojure gives `1/2` and `1/4`.
- `quot` and `rem` truncate toward zero and `mod` takes the divisor's sign, so
  `(rem -7 2)` is -1 and `(mod -7 2)` is 1.
- With no arguments `+` gives 0 and `*` gives 1; unary `-` negates and unary `/`
  takes the reciprocal.
- As first-class values the functions check their arity, evaluate their
  arguments once in source order, and keep their behaviour in captures after
  `suss.core` is rebound.

The auto-promoting `+'`, `-'`, `*'`, `inc'` and `dec'` need BigInt (#338).

Tests: `crates/suss-cli/tests/clojure_equality_bitwise.rs` and the jank
clojure-test-suite's arithmetic namespaces
([oracle](../../tests/oracle/clojure-test-suite-known-failures.json)).
