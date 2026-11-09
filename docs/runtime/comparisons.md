# Comparisons

`<`, `<=`, `>`, `>=` and `==` compare numbers as Clojure 1.12 does
([ADR-0003](../adr/0003-clojure-numeric-tower.md)), across longs and doubles:
`(< 1 2.5)` is true and `(== 1 1.0)` is true, while `(= 1 1.0)` is false
([numeric hashing](numeric-hashing.md)).

- Any operand that isn't a number throws, for example "< requires number
  operands", where the JVM throws a `ClassCastException` or a
  `NullPointerException`. Strings compare with `compare`, not `<`.
- NaN compares false with everything, itself included, and `-0.0` equals
  `0.0`: `(> 0.0 -0.0)` is false and `(<= -0.0 0.0)` true.
- With one argument each returns true without comparing; with more it checks
  each adjacent pair, left to right, and stops at the first false.
- As first-class values they check arity and evaluate their arguments once in
  source order. The core macros expand a direct call into a chain of binary
  comparisons; a lexical binding of the same name hides the macro.

`compare` orders numbers by value, so `(compare 1 1.0)` is 0; `max` and `min`
return the greater or lesser operand, and NaN if either is NaN.

Tests: `crates/suss-cli/tests/portable_comparisons.rs` and
`crates/suss-cli/tests/comparison_operand_evaluation.rs`.
