# Arithmetic

`+`, `-`, `*`, `/`, `inc`, `dec`, `quot`, `rem` and `mod` follow Clojure 1.12.6
for longs, doubles, BigInts, Ratios and BigDecimals
([ADR-0003](../adr/0003-clojure-numeric-tower.md)). The operations use the shared
GC runtime and numeric helper; the compiler inlines them where it can. They are
also first-class functions in
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
- `/` of two longs that divide exactly gives a long; a non-integral quotient
  is an exact Ratio. `(/ 1 2)` gives `1/2` and `(/ 4)` gives `1/4`.
  Integer division by zero throws "Divide by zero".
- `quot` and `rem` truncate toward zero and `mod` takes the divisor's sign, so
  `(rem -7 2)` is -1 and `(mod -7 2)` is 1.
- With no arguments `+` gives 0 and `*` gives 1; unary `-` negates and unary `/`
  takes the generic reciprocal, including its zero check. Inlined variadic
  division is a nested left fold: a division error prevents evaluation of a
  later operand. Both operands of each binary operation evaluate once in order.
- As first-class values the functions check their arity, evaluate their
  arguments once in source order, and keep their behaviour in captures after
  `suss.core` is rebound.

The auto-promoting `+'`, `-'`, `*'`, `inc'` and `dec'` promote integer overflow
to BigInt: `(+' Long/MAX_VALUE 1)` gives `9223372036854775808N`. A BigInt result
stays a BigInt, so `(+ 1N 1)` is `2N`. Ratios normalize their sign and common
factors; a result that normalizes to an integer becomes a BigInt, as
`(+ 1/3 2/3)` gives `1N`.

BigDecimals retain an arbitrary-precision integer coefficient and a signed
32-bit scale. Arithmetic follows Clojure's scale rules: `(* 1.5M 2)` gives
`3.0M`. Division without a terminating decimal expansion throws unless
`with-precision` supplies the dynamic `*math-context*`; rounding modes and
precision use that actual context. The category order is long, BigInt, Ratio,
BigDecimal, then double, with double contagious over the exact categories.
`numerator`, `denominator`, `rationalize`, `bigint`, `biginteger`, `bigdec` and
the numeric predicates retain their Clojure semantics.

Exact arithmetic, source overloads, raw/boxed fallback and Decimal context
observations are retained in [BigInt evidence](../../results/m12-bigint-integers/README.md),
[Ratio evidence](../../results/m12-ratio-values/README.md) and
[Decimal evidence](../../results/m12-decimal-integration/README.md).
The [measured implementation placement](../../results/m12-helper-multiply-final/README.md)
records the GC/helper choice; native and AOT conformance retain their own
validation receipts.

Tests: `crates/suss-cli/tests/clojure_equality_bitwise.rs` and the jank
clojure-test-suite's arithmetic namespaces
([oracle](../../tests/oracle/clojure-test-suite-known-failures.json)).

Source overload evidence is in `results/m12-division-overloads`, with a fresh
JVM1.12.6 oracle and Runtime/Macro/AOT regressions in
`crates/suss-cli/tests/clojure_division_overloads.rs`.
