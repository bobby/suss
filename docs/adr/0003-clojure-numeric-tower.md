# ADR-0003: Clojure's numeric tower

Status: **Accepted** on 2026-11-06, with the answers in
[resolved questions](#resolved-questions).
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
| i31 small integer, boxed `i64` fallback, inline (untagged) | 1.9 |
| i31 small integer with the low-bit tag chosen below (second set of runs) | 2.2–2.4 |
| Boxed `i64` struct with overflow check | 3.2 |
| Boxed `f64` struct (today) | 3.8–4.1 |
| Out-of-line polymorphic `+`, i31 operands | 2.6 |
| Out-of-line polymorphic `+`, boxed operands | 10.4–11.7 |

So a boxed long with overflow checking costs no more than today's boxed double;
small integers in i31 halve that and allocate nothing; and unboxed locals, which
type inference or `^long` hints would allow, are 1.6–6× faster again (2.7–11×
faster than a boxed long). Under the copying collector, the tag test of the
encoding chosen below costs 0.3–0.4 ns per addition, measured in a second set
of runs (`run_copying_live*`) on a busier machine.

A live heap changes the picture for boxes only. With 10⁶ live objects, a boxed
addition under the copying collector costs 21–22 ns instead of 3–4.5, and each
collection pauses for about 25 ms; at 4·10⁶ live objects, 22–24.5 ns and 67 ms.
Unboxed and i31 arithmetic allocate nothing and keep their speed. wasmtime's
deferred-reference-counting collector calls into the host for every
`struct.new`. With a small heap a box costs about 110–150 ns; with 10⁶ live
objects, about 22 ns, as under copying. Why the cost falls wasn't
investigated.

## Decision

Adopt Clojure 1.12's numeric semantics and types, with the jank suite's
`:default` expectations and JVM Clojure as the oracle. Represent them with
WasmGC's native integer and float types, unboxed wherever the compiler can, and
map WIT's numeric types onto them directly.

### Types

| Clojure type | Category | Suss value |
| --- | --- | --- |
| `Long` | integer | Signed 64-bit integer |
| `Double` | floating | IEEE-754 binary64, including NaN, infinities and -0.0 |
| `BigInt` | integer | Arbitrary-precision integer |
| `Ratio` | ratio | Normalised fraction of integers, denominator > 1 |
| `BigDecimal` | decimal | Arbitrary-precision unscaled integer and 32-bit scale, as Java's `BigDecimal` |

`Integer`, `Short`, `Byte` and `Float` have no separate value types, as in
jank, which has only 64-bit integers and doubles:

- `int`, `short` and `byte` check the range and return a long, which Clojure's
  `=`, `==` and `hash` already treat as the same value. `(int 1e10)` throws
  "Value out of range for int", as in Clojure. `unchecked-int`, `unchecked-short`
  and `unchecked-byte` wrap instead.
- `float` rounds to binary32 precision and returns a double, so
  `(= (float 0.1) 0.1)` is false as in Clojure, but the result prints as a
  double.
- `class` and `type` name the five types above, so `(instance? Integer (int 1))`
  and the suite's few `:clj` class checks have no Suss equivalent.

Narrow types are storage types instead. `byte-array`, `short-array`,
`int-array` and `float-array` are packed WasmGC arrays of `i8`, `i16`, `i32` and
`f32`, and `long-array` and `double-array` arrays of `i64` and `f64`. Elements
are signed, as in Clojure. `aget` returns an element as a long or double.
`aset` on a narrow integer array checks the range and throws "Value out of
range for byte: 200", as `(byte 200)` does, and `aset` on a `float-array`
rounds. This differs from Clojure, where `aget` returns a boxed `Byte` or
`Float`, `(aset (int-array 1) 0 10000000000)` throws "integer overflow", and
`aset` on a `byte-array` with a long fails reflection.

### Arithmetic

- `+`, `-` (including unary), `*`, `inc` and `dec` on longs throw a language
  error, "long overflow", on overflow. `+'`, `-'`, `*'`, `inc'` and `dec'`
  promote to BigInt. `unchecked-negate` and the other `unchecked-*` functions
  wrap. As in Clojure, `(abs Long/MIN_VALUE)` returns it unchanged and
  `(quot Long/MIN_VALUE -1)` wraps.
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
  unselected reader-conditional branches are read as Clojure reads them, so
  only a malformed literal, such as `1/0`, fails there.
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
| `list<T>` of a numeric `T` | A persistent vector, as for any list; when leaving Suss, a packed array is also accepted and copied without boxing (see below) |

A list leaving Suss accepts these arrays as well as vectors:

| WIT list | Array | Element rule |
| --- | --- | --- |
| `list<s8>`, `list<s16>`, `list<s32>`, `list<s64>` | `byte-array`, `short-array`, `int-array`, `long-array` | Copied as is |
| `list<u8>` | `byte-array` | Each byte's bits, so `-1` sends 255, as Java's byte I/O does |
| `list<f32>`, `list<f64>` | `float-array`, `double-array` | Copied as is |

`list<u16>`, `list<u32>` and `list<u64>` take vectors only.

`WitSigned64`, `WitUnsigned64`, `wit-s64`, `wit-u64` and `wit-to-number` are
retired.

### Representation

Numbers are unboxed wherever possible:

- **Unboxed locals.** The compiler keeps a long or double as a raw `i64` or
  `f64` Wasm value whenever it knows the type: from literals, arithmetic
  results, `loop` bindings, Clojure's `^long` and `^double` hints, and calls to
  core functions with known result types. A value is boxed only where it
  escapes: stored in a collection or a closure, passed to a function the
  compiler can't see, or returned from one called through `IFn`. A function
  with `^long` or `^double` parameters or result gets a direct unboxed entry
  point beside its generic one, as Clojure's primitive interfaces do, and
  callers the compiler can resolve use it.
- **Inline small longs.** A long held as a `Value` reference whose value fits in
  31 signed bits (−2³⁰ to 2³⁰ − 1) is that i31 value; other longs are a boxed
  `i64` struct. Each long has exactly one representation, so equality and
  hashing never compare a box with an i31. This is part of the first ABI
  change, not a later stage.
- **Sentinels outside i31.** nil is `ref.null`, and false, true and undefined
  are singleton structs shared by every fragment, so no i31 is anything but a
  long. #335 measured this against the tagged encoding first proposed here,
  which kept the even i31 values as sentinels and held a long `v` as
  `(v << 1) | 1` ([experiments/numeric-representation](../../experiments/numeric-representation/README.md#sentinel-designs-335)):
  small-long addition is 21% faster (1.87 ns against 2.38) and a counting
  loop through polymorphic `<` and `inc` 12% faster, while truthiness tests
  cost the same within noise. A nil struct measured no faster than
  `ref.null`; `Value` is already a nullable `eqref`, and a null is a constant
  that default-initialized arrays and structs already hold.
- A boxed double is an `f64` struct.
- Shared GC runtime ABI version 3 adds the new types to the shared recursive group.
- A BigInt holds a sign and a magnitude array; a Ratio two integers; a BigDecimal
  an integer and an `i32` scale. Which operations live in generated runtime code
  and which in the no_std numeric helper is decided by measurement in the first
  work package.

### Collector

The collector is a choice only on the native wasmtime host; in a browser or
Node, the JavaScript engine's collector manages WasmGC objects.

- Suss configures `Collector::Copying` explicitly instead of relying on
  `Collector::Auto`, which selects it in wasmtime 49.0.1 but "may change over
  time". Copying allocates in compiled code without calling the host, collects cycles and has good throughput,
  but stops the program for each collection, for time proportional to the live
  heap (about 25 ms at 10⁶ live objects), and uses half of the GC heap at a
  time.
- The null collector, which never frees and traps when the heap is full, is an
  opt-in for short-lived runs, such as one-shot scripts or a component instance
  per request.
- The deferred-reference-counting collector is not offered: per wasmtime's
  documentation it "cannot currently collect cycles", which self-referencing
  functions (a named `fn`, `letfn`) and atoms whose value refers back to the
  atom create. It also costs about 110–150 ns per allocation with a small
  heap.
- wasmtime's own documentation of `Collector::Copying` still says it "is still
  under construction and is not yet functional", in the 49.0.1, 50.0.0 and
  51.0.0-rc.1 crates published on crates.io, although `Auto` selects it (with
  the default `gc-copying` feature) and Suss's tests and these benchmarks run
  on it today. Each wasmtime upgrade rechecks its status and release notes.
- wasmtime 49.0.1 grows the GC heap only when a collection leaves less free
  space than the pending allocation needs, so a heap that live data nearly
  fills is collected every few allocations. #335 measured boxed addition at
  4 ns with no live heap, 20 ns with 2,048 live objects and 131 ns with 65,536
  under the default initial heap; with a 64 MiB initial heap
  (`Config::gc_heap_initial_size`), 4–5 ns at both, at the cost of 64 MiB more
  peak RSS, since the copying collector touches all of its heap
  ([GC heap growth](../../experiments/numeric-representation/README.md#gc-heap-growth)).
  Much of the live-heap cost above is this policy. Suss sets the initial size
  with the collector, and #335 records the value it chooses.
- Unboxed math reduces allocation, and so collector pressure, whichever
  collector runs: with 10⁶ live objects, a boxed addition costs about ten
  times an i31 one.

### Oracle and reader features

- Suss's reader features are `:suss` and `:default`, taking the first matching
  branch in source order, for user code, libraries and the suite alike. The
  `:cljs` feature is dropped: a `:cljs` branch assumes JavaScript numbers and
  host interop.
- Suss doesn't claim `:clj` either. A `.cljc` file treats `:clj` as the JVM.
  About half of the suite's 33 `:clj` branches check JVM classes or use
  interop (`java.lang.Long`, `clojure.lang.Atom`, `Long/MAX_VALUE`,
  `(Object.)`, `Thread/sleep`); the rest state JVM-specific behaviour, some of
  which Suss follows (next item).
- The suite's `:default` branches (571, against 545 `:cljs`) already state
  Clojure's behaviour; `:cljs`, `:cljr` and `:lpy` mark where other platforms
  differ. Where only a `:clj` branch states a rule Suss follows, such as
  `(abs Long/MIN_VALUE)` or `read-string` promoting to BigInt, a `:suss`
  branch is added to the suite, as jank added `:jank` branches, and offered
  upstream.
- The suite's expected values come from JVM Clojure 1.12.6, a development-only
  dependency like Node is today, instead of ClojureScript recordings and a
  corrections file.

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
- A `.cljc` library whose reader conditionals have only `:clj` and `:cljs`
  branches, and no `:default`, reads as if those forms were absent, as it does
  on any platform whose feature isn't listed. Such libraries need `:suss` or
  `:default` branches.
- Unboxed locals and primitive entry points, which the earlier draft of this
  ADR deferred to M11, are part of this work: the compiler needs local type
  inference for numbers, and each `^long`/`^double` function has two entry
  points.
- Arbitrary-precision arithmetic needs its own implementation in WasmGC or the
  numeric helper; there is no host library to call.
- About 75 runtime sites test or cast i31 values on the assumption that an i31
  is nil, a boolean or undefined, and some runtime state (stream and async
  status, named-property kinds) stores i31 values. With the sentinels outside
  i31, each of those tests must test for the sentinel structs instead, and
  those state slots, whose values now read as longs, must never be observable
  as values.

## Alternatives considered

- **Keep binary64 (ClojureScript's model).** No migration, but it contradicts
  the oracle, keeps 64-bit WIT integers in wrappers, and leaves the suite's
  `:default` numeric expectations unmet.
- **Longs and doubles only.** Smaller, and covers WIT and most programs, but `/`
  of integers then has no exact result, and ratios, `N`, `M`, `+'` and
  `with-precision` remain unsupported. It could be a first stage of this decision,
  not an end state.
- **jank's fixed-precision decimal.** `cpp_dec_float<100>` has no scale, so
  printing `1.50M`, scale-dependent results (`(* 1.5M 2)` is `3.0M`), the
  non-terminating `/` error and `with-precision` rounding couldn't follow
  Clojure.
- **Boxed longs without the i31 fast path, or a boxed-only first stage.**
  Simpler, and costs no more than today, but gives up the 2× for small
  integers and allocates on every operation.
- **Unboxed locals later, under M11.** Smaller first stages, but every
  arithmetic operation would allocate or tag until then, and the ABI and
  compiler would change twice.
- **Narrow boxed `Integer`, `Short`, `Byte` and `Float` values.** Would let
  `class` and `instance?` match the JVM, but Clojure widens them on almost
  every operation, and they add categories to equality, hashing and contagion
  for little observable benefit.
- **Claiming `:clj`, or keeping `:cljs`.** Either selects branches written
  for another host: JVM classes, or JavaScript numbers and interop.
- **Deferred reference counting.** Leaks cycles and costs about 30× copying
  per allocation with a small heap.

## Staging

[M12: Clojure numeric tower](https://github.com/bobby/suss/milestone/14) holds
one work package per stage:

1. Reader: read every Clojure numeric literal, including in unselected branches;
   this can land first, keeping binary64 values and reporting unsupported
   literals only when they are evaluated.
2. Reader features `:suss` and `:default` only, with the suite's `:suss`
   branches; JVM Clojure recordings as the suite oracle.
3. Runtime ABI v3: long and double types, i31 small longs with the sentinels
   moved out of i31, the explicit collector, overflow checks, and the
   reader, printer, EDN and macro data for longs and doubles.
4. Equality, hashing, comparison, bitwise operations and coercions for longs and
   doubles; packed primitive arrays; WIT integer and list mapping, retiring the
   wrappers.
5. Unboxed locals: numeric type inference, `^long` and `^double` hints, and
   primitive entry points.
6. BigInt and Ratio, `+'`-style promotion, `numerator`, `denominator`,
   `rationalize` and the predicates.
7. BigDecimal and `with-precision`.

## Resolved questions

Answered on 2026-11-06 before acceptance:

1. **Reader features.** Suss uses `:suss` and `:default`, drops `:cljs` and
   doesn't claim `:clj`, in user code and the suite alike.
2. **Unboxed math.** Wherever possible: i31 small longs from the first ABI
   change (untagged, with the sentinels moved out of i31, as #335 measured), and
   unboxed locals and primitive entry points in this milestone rather than
   later.
3. **Narrow types.** No narrow boxed values; `int`, `short`, `byte` and `float`
   return longs and doubles, and narrow types exist as packed array elements
   and at the WIT boundary.
4. **Collector.** Copying, configured explicitly, with null as an opt-in for
   short-lived runs and no reference counting; rechecked at each wasmtime
   upgrade.
