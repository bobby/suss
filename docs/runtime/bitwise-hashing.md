# Bit operations and numeric casts

Numbers follow Clojure 1.12
([ADR-0003](../adr/0003-clojure-numeric-tower.md), #336). This page replaces
the 32-bit ClojureScript notes that preceded that decision.

## Bit operations

`bit-and`, `bit-or`, `bit-xor`, `bit-and-not`, `bit-not`, `bit-clear`,
`bit-flip`, `bit-set`, `bit-test`, `bit-shift-left`, `bit-shift-right` and
`unsigned-bit-shift-right` take longs and operate on 64 bits, as Clojure's do.

- Any other operand throws: a double throws "bit operation not supported for:
  double", as Clojure's `bitOpsCast` rejects a `Double`, and nil or a
  non-number throws too.
- Shift counts and bit indexes use their low six bits, as Java's shifts do:
  `(bit-shift-left 1 65)` is 2 and `(bit-set 0 64)` is 1.
- `unsigned-bit-shift-right` is a logical 64-bit shift:
  `(unsigned-bit-shift-right -1 1)` is 9223372036854775807.
- `bit-shift-right-zero-fill`, ClojureScript's name, is the same operation.

Direct calls lower inline; variadic direct calls nest pairs. The first-class
values live in canonical `suss.core` cells and use universal invocation. A
variadic body computes its first pair, then reads its live cell once for the
reducer that folds the rest, so a saved function keeps its body while that
lookup sees redefinitions.

ClojureScript's `imul` and the runtime's ToInt32, `coerce-int32`, are not
public. Shipped core keeps them, with 32-bit forms of the operations above, for
its hashing ([numeric equality and hashing](numeric-hashing.md)). Their
provenance stays in [bitwise-primitives.json](../compatibility/bitwise-primitives.json),
checked by `scripts/bitwise_provenance.py`.

## Casts

There are no narrow boxed values; casts return longs and doubles. Core's casts
call the private `suss.bootstrap/cast` with an index into the runtime's table,
and each table entry is a runtime function `number-cast-<name>`.

| Cast | Long | Double |
| --- | --- | --- |
| `long` | as is | truncated; beyond the long range throws |
| `int`, `short`, `byte` | as is if in range, else throws | truncated if in range, else throws |
| `unchecked-long` | as is | truncated, saturating |
| `unchecked-int`, `unchecked-short`, `unchecked-byte` | low bits, sign-extended | truncated to an int, saturating, then its low bits |
| `float` | rounded to binary32, as a double | the same; beyond the float range throws |
| `unchecked-float` | rounded to binary32, as a double | the same, unchecked |
| `double`, `unchecked-double` | as a double | as is |

Each matches JVM Clojure 1.12's cast:

- The range check compares the double itself, before truncation, so
  `(int 2147483647.5)` and `(short 32767.5)` throw.
- An out-of-range double throws with the printed value: "Value out of range for
  int: 3.0E9", "Value out of range for long: 1.0E19", "Value out of range for
  float: Infinity". The long cast's upper limit, 2^63 itself, saturates to
  9223372036854775807.
- An out-of-range long passed to `short` or `byte` throws the same way:
  "Value out of range for byte: 128".
- `int` of an out-of-range long throws "integer overflow", as Clojure's
  `Math.toIntExact` does.
- NaN casts to 0 with `long` and `int`; `short` and `byte` reject it.
- A non-number throws "int requires a number operand", and likewise for the
  others.

`float` returns a double, so `(= (float 0.1) 0.1)` is false, as in Clojure,
but `(float 0.1)` prints as 0.10000000149011612 where Clojure prints a `Float`
as 0.1.

The `-int` arithmetic functions, such as `unchecked-add-int`, wrap their result
to 32 bits.
