# ADR-0003: Clojure's numeric tower

Status: **Proposed**. The direction, Clojure's numeric semantics, was decided on
2026-11-06 (design decision log); the representation, WIT mapping, staging and
[open questions](#open-questions) await acceptance.
Date: 2026-11-06.
Issue: [#295](https://github.com/bobby/suss/issues/295).

## Context

Suss's behaviour contract is Clojure, checked by the vendored jank
clojure-test-suite (2026-10-25 design decision). Numbers are the largest place
where Suss still follows ClojureScript instead. Every Suss number is one
IEEE-754 binary64 value, boxed in the shared GC group's `NUMBER` struct
(`runtime_abi.rs`). The reader rejects ratios and the `N` and `M` suffixes, `=`
is `==`, and WIT `s64`/`u64` values travel in separate `WitSigned64` and
`WitUnsigned64` wrapper types ([exact integers](../runtime/portable-exact-integers.md)).

This disagrees with the oracle in ways programs can see. Clojure 1.12.6 gives:

| Expression | Clojure | Suss today |
| --- | --- | --- |
| `(+ 9223372036854775807 1)` | throws "long overflow" | `9223372036854776000` |
| `(+' Long/MAX_VALUE 1)` | `9223372036854775808N` | unsupported |
| `(/ 1 3)`, `(/ 6 3)` | `1/3`, `2` | `0.3333333333333333`, `2` |
| `(/ 1 0)` | throws "Divide by zero" | `##Inf` |
| `(= 1 1.0)`, `(== 1 1.0)` | `false`, `true` | `true`, `true` |
| `(= 1 1N)`, `(= 1 1M)` | `true`, `false` | unsupported |
| `1.0`, `1e21` printed | `1.0`, `1.0E21` | `1`, `1e+21` |
| `(bit-shift-left 1 63)` | `-9223372036854775808` | `-2147483648` (32-bit) |
| `(integer? 1.0)` | `false` | `true` |
| `(hash 1)`, `(hash 1.0)` | `1392991556`, `1072693248` | `1`, `1` |

The test suite is blocked by this gap. The Suss reader rejects ratio, `N` and `M`
literals even inside reader-conditional branches it doesn't select, which stops
87 suite namespaces, holding 2,807 recorded assertions, from loading. Many of
these aren't about numbers (`case`, `list`, `seq`, the ident predicates). About
78 suite files carry 1,815 numeric assertions under `:cljs` alone. The
`:default` branches carry more, because the ratio, BigInt and BigDecimal tests
are `:cljs nil`. jank takes those `:default` (Clojure) expectations nearly
everywhere. The suite's recorded expectations come from ClojureScript under Node,
which can't express longs, doubles, ratios or big numbers as distinct values.

jank implements the tower natively: `integer` is an `i64`, `real` an `f64`, and
`ratio`, `big_integer` and `big_decimal` use Boost.Multiprecision
(`cpp_int`, and `cpp_dec_float<100>` for decimals). Its `+'` family promotes to
big integers on overflow and division by zero throws.

WIT's numeric types are `s8`–`s64`, `u8`–`u64`, `f32` and `f64`. A 64-bit long
holds every WIT integer except `u64` above 2^63 − 1. The wrapper types exist
only because binary64 can't hold 64-bit integers.

### Measured cost in WasmGC

A microbenchmark on the workspace's wasmtime 49.0.1 configuration
([experiments/numeric-representation](../../experiments/numeric-representation/README.md))
adds integers through `eqref` values, in ns per addition (copying collector,
which wasmtime selects when no collector is configured, as in Suss today):

| Representation | ns |
| --- | --- |
| Unboxed `i64` local, no overflow check (lower bound) | 0.3 |
| Unboxed `i64` with Clojure's overflow check | 1.2 |
| i31 small integer, boxed `i64` fallback, inline | 1.9 |
| Boxed `i64` struct with overflow check | 3.2 |
| Boxed `f64` struct (today) | 3.8–4.1 |
| Out-of-line polymorphic `+`, i31 operands | 2.6 |
| Out-of-line polymorphic `+`, boxed operands | 10.4–11.7 |

So a boxed long with overflow checking costs no more than today's boxed double;
small integers in i31 halve that and allocate nothing; and unboxed locals, which
type inference or `^long` hints would allow, are another 3–10× faster. The
collector matters more than the representation: under wasmtime's
deferred-reference-counting collector every box costs about 113 ns, 30× the
copying collector, because each `struct.new` calls into the host.

## Decision

Adopt Clojure 1.12's numeric semantics and types, with the jank suite's
`:default` expectations and JVM Clojure as the oracle. Represent them with
WasmGC's native integer and float types, and map WIT's numeric types onto them
directly. The sections below are the proposed contract; the
[open questions](#open-questions) need answers before acceptance.

### Types

| Clojure type | Category | Suss value |
| --- | --- | --- |
| `Long` | integer | Signed 64-bit integer |
| `Double` | floating | IEEE-754 binary64, including NaN, infinities and -0.0 |
| `BigInt` | integer | Arbitrary-precision integer |
| `Ratio` | ratio | Normalised fraction of integers, denominator > 1 |
| `BigDecimal` | decimal | Arbitrary-precision unscaled integer and 32-bit scale, as Java's `BigDecimal` |

`Integer`, `Short` and `Byte` have no separate types: `int`, `short` and `byte`
check the range, as Clojure does, and return a long, which Clojure's `=`, `==`
and `hash` already treat as the same value. `float` rounds to binary32 precision
and returns a double, so `(= (float 0.1) 0.1)` is false as in Clojure, but the
result prints as a double. `class` and `type` name the five types above.

### Arithmetic

- `+`, `-`, `*`, `inc`, `dec`, `negate` and `abs` on longs throw
  `ArithmeticException`-equivalent language errors ("long overflow") on
  overflow. `+'`, `-'`, `*'`, `inc'` and `dec'` promote to BigInt.
  `unchecked-*` wraps. `(quot Long/MIN_VALUE -1)` wraps, as in Clojure.
- Contagion follows Clojure's categories: long → BigInt → Ratio → BigDecimal →
  double, with double contagious over every other type. A BigInt result that fits
  in a long stays a BigInt (`(+ 1N 1)` is `2N`); a Ratio that normalises to an
  integer becomes a BigInt (`(+ 1/3 2/3)` is `1N`), and `/` of two longs that
  divide evenly is a long.
- `/` of integers is exact. Integer division by zero throws "Divide by zero";
  double division follows IEEE-754. A BigDecimal quotient without an exact
  result throws "Non-terminating decimal expansion" unless `with-precision`
  sets a `*math-context*`.
- `quot`, `rem` and `mod` follow Clojure for every category, including doubles.
- Arithmetic, comparison and the sign predicates keep checking their operands
  (design section 4): nil and non-numbers throw.

### Equality, hashing and comparison

- `=` is true only within a category, with numeric equality inside it: `(= 1 1N)`
  is true, `(= 1 1.0)` and `(= 1 1M)` are false, and `(= 1.0M 1.00M)` is true.
- `==` compares numeric value across categories.
- `hash` is Clojure's `hasheq`: a long and a BigInt in long range hash as the long;
  doubles, ratios and decimals hash as Clojure does.
- `compare` orders all numbers by value, across categories.

### Bitwise operations

Bit operations take longs and operate on 64 bits; any other number throws, as
in Clojure. Shift counts use the low six bits.
`unsigned-bit-shift-right` is a logical 64-bit shift. The 32-bit ClojureScript
coercions, `coerce-int32` and `imul`, leave the public contract.

### Reader and printer

- The reader accepts Clojure's numeric literals: decimal, hexadecimal, octal
  (`017` is 15), radix (`36rZ`), `N` and `M` suffixes, ratios, and doubles.
  An integer literal beyond the long range reads as a BigInt. Literals in
  unselected reader-conditional branches are read, not rejected.
- The printer prints as Clojure: `1`, `1.0`, `1N`, `1M`, `1.50M`, `1/2`, and
  doubles in Java's `Double.toString` format (`1.0E21`, `1.0E-4`, `1234567.0`),
  with `##NaN`, `##Inf` and `##-Inf`. `str` drops the `N` and `M` suffixes.
- EDN, `read-string` and macro data carry the same types.

### WIT boundary

| WIT type | Suss value |
| --- | --- |
| `s8`–`s64`, `u8`–`u32` | Long, range-checked when leaving Suss |
| `u64` | Long up to 2^63 − 1, BigInt above; range-checked when leaving Suss |
| `f32`, `f64` | Double; `f32` rounds when leaving Suss |

`WitSigned64`, `WitUnsigned64`, `wit-s64`, `wit-u64` and `wit-to-number` are
retired.

### Representation

- Shared GC runtime ABI version 3 adds the new types to the shared recursive group.
- A long whose value fits 30 bits is an i31 reference with its low bit set
  (`(v << 1) | 1`), so the even i31 values keep their meaning as the nil, false,
  true and undefined sentinels. Other longs are a boxed `i64` struct. Each long
  has exactly one representation, so equality and hashing never compare a box
  with an i31.
- A double stays a boxed `f64` struct.
- A BigInt holds a sign and a magnitude array; a Ratio two integers; a BigDecimal
  an integer and an `i32` scale. Which operations live in generated runtime code
  and which in the no_std numeric helper is decided by measurement in the first
  work package.
- Suss configures wasmtime's collector explicitly, rather than relying on
  `Collector::Auto`, and records the choice with its measurement.
- Compiled code keeps the generic boxed path. Unboxed `i64` and `f64` locals,
  from inference and Clojure's `^long` and `^double` hints, are a later
  optimisation under [M11](https://github.com/bobby/suss/milestone/13).

### Oracle and reader features

- The suite's expected values come from JVM Clojure 1.12.6, a development-only
  dependency like Node is today, instead of ClojureScript recordings and a
  corrections file.
- The suite runs with Suss selecting `:default` branches, not `:cljs`.

## Consequences

- The jank suite's numeric files can be run as written, and the 87 namespaces the
  reader blocks today can load.
- WIT 64-bit integers become ordinary numbers, and the wrapper types go away.
- It is a large change: roughly 6–10k lines across 40–60 files, by the
  implementation inventory. It touches the reader, EDN and macro data, HIR literal
  and type kinds, constant folding, the runtime ABI and every fragment's
  manifest, the numeric helper, `=`, `hash` and `compare`, the printer, bitwise
  and coercion intrinsics, core's numeric functions, AOT WIT adapters, and every
  recorded oracle observation that contains a number.
- Programs written for ClojureScript's numbers change behaviour: `(= 1 1.0)`,
  `(/ 1 3)`, division by zero, 32-bit bit operations and number printing all
  differ.
- Arbitrary-precision arithmetic needs its own implementation in WasmGC or the
  numeric helper; there is no host library to call.

## Alternatives considered

- **Keep binary64 (ClojureScript's model).** No migration, but it contradicts
  the oracle, keeps 64-bit WIT integers in wrappers, and leaves the suite's
  `:default` numeric expectations unmet.
- **Longs and doubles only.** Smaller, and covers WIT and most programs, but `/`
  of integers then has no exact result, and ratios, `N`, `M`, `+'` and
  `with-precision` remain unsupported. It could be a first stage of this decision,
  not an end state.
- **jank's fixed-precision decimal.** `cpp_dec_float<100>` has no scale, so
  `1.50M`, `(= 1.0M 1.00M)`'s semantics and `with-precision` couldn't follow
  Clojure.
- **Boxed longs without the i31 fast path.** Simpler, and costs no more than
  today, but gives up the 2× for small integers and allocates on every operation.

## Staging

A dedicated milestone, created if this proposal is accepted, could hold:

1. Reader: read every Clojure numeric literal, including in unselected branches;
   this can land first, keeping binary64 values and reporting unsupported
   literals only when they are evaluated.
2. Runtime ABI v3: long and double types, i31 longs, collector choice, overflow
   checks, and the reader, printer, EDN and macro data for longs and doubles.
3. Equality, hashing, comparison, bitwise operations and coercions for longs and
   doubles; WIT integer mapping and retiring the wrappers.
4. BigInt and Ratio, `+'`-style promotion, `numerator`, `denominator`,
   `rationalize` and the predicates.
5. BigDecimal and `with-precision`.
6. Oracle: JVM Clojure recordings for the suite and `:default` branch selection.

## Open questions

1. Should Suss drop the `:cljs` reader feature for user code too, not only for
   the suite? With Clojure numerics, a `.cljc` library's `:cljs` branch would
   assume JavaScript numbers.
2. Is i31 encoding of small longs wanted from the start, or after a boxed-only
   first stage?
3. Should `int`, `short`, `byte` and `float` keep returning longs and doubles,
   or should Suss add Clojure's narrow boxed types?
4. Which collector: copying (fast allocation, stop-the-world, no generations in
   wasmtime 49) or another?
