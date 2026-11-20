# Unboxed numeric locals

Inside a compiled function, the compiler keeps a long as a raw `i64` Wasm
local and a double as a raw `f64`, and boxes a number only where it escapes
([ADR-0003](../adr/0003-clojure-numeric-tower.md) "Representation", #337).
Results are those of the boxed path in every case: the same values, the same
overflow checks, and the same exceptions with the same messages.

## What stays raw

After lowering to IR, `emit/unboxed.rs` infers each value's representation:

- **Literals.** A long or double literal that feeds raw arithmetic, a raw
  comparison or a raw loop local.
- **Arithmetic.** `+`, `-` and `*` of longs give a raw long; any double operand
  gives a raw double. `/` gives a raw double when an operand is a double; a
  quotient of two longs stays boxed, since only an exact one is a long. `inc`
  and `dec` are `+` and `-` of 1. Core's coercing `+`, `-` and `*` also keep
  two known numeric operands raw, matching the helpers' checked numeric route.
  Unknown operands, guarded replacement results, unary coercions and core's
  division retain their original boxed path.
- **`let` and `loop`.** A `let` binding is the value it's bound to. A `loop`
  local, and an `if` result that joins branches, are raw when every value
  reaching them is the same raw kind. A local that a `recur` gives another
  kind, as in `(loop [i 0] (recur (+ i 0.5)))`, stays boxed, as Clojure
  boxes a primitive loop local whose `recur` doesn't match it.
- **Comparisons.** `<`, `<=`, `>`, `>=`, `==`, `zero?`, `pos?` and `neg?` of raw
  numbers give a raw truth value, which `if` and `loop` test directly. Two
  longs compare exactly; a long and a double compare as doubles, as in the
  runtime. A raw long compared with any other value checks whether that value
  is an i31 and compares inline if so; otherwise it asks the runtime.
  Core's coercing comparisons use this route for two raw numeric operands;
  strings and other representations retain the coercing helper.

- **Bitwise operations.** `bit-and`, `bit-or` and `bit-xor` of two longs give a
  raw long (#387). `bit-test` of two raw longs gives a raw truth value; the
  shift count uses its low six bits, including for negative counts, as Java's
  long shift does. The truth value is boxed where it escapes. Other operand
  representations take the runtime operation, which throws for a double as
  Clojure does.
  Core's private 32-bit operations also stay raw for long operands, using their
  low 32 bits, five-bit shift counts and the original signed result extension
  (unsigned for zero-fill right shift). Unknown operands keep their existing
  ToInt32 conversion path.
- **Hashes and identity.** `hash` of a long gives a raw long: an i31 or raw long
  is hashed inline, as `number-hash` hashes it, and core's `hash`, which always
  returns an integer, is cast to a long (#387). `identical?`, `=` of small
  integers and the small-integer test read raw operands without boxing them:
  a raw long is identical to an i31 or long box of its value, and a raw
  double to a boxed double that compares equal.
- **Hinted arrays.** `(aget ^longs a i)` and `(aset ^longs a i v)`, and the
  other packed hints, cast `a` to a packed array of the hinted kind (#389).
  That's a checked cast, as Clojure's hint is: a wrong hint throws a language
  error such as "aget ^longs requires a long array", as a `^long` parameter's
  cast does (#385). The element is then a raw long, or a raw double for
  `^doubles` and `^floats`. A raw long index and a raw stored value of the
  element's kind are used directly, and `aset` returns a raw stored value raw.
  An index the inline path can't use takes the generic path, which converts
  it or reports it as before.
- **Hinted parameters.** A parameter tagged `^long` or `^double` is, in the
  body, Clojure's checked `longCast` or `doubleCast` of the argument (#385): an
  i31 or a boxed number of that kind is read inline, and anything else goes
  through the runtime's `number-cast-long` or `number-cast-double`, which
  converts a double in range or throws. The result is a raw `i64` or `f64`, so
  arithmetic and loops seeded from it stay unboxed beyond i31, and `recur`
  rebinds it. A WIT export's numeric parameters are treated as hinted
  ([WIT numbers](wit-numbers.md), "Primitive export parameters").
- **Core coercions.** `long`, `double`, `unchecked-long` and
  `unchecked-double` produce raw results. A raw long stays raw or widens to
  a double; a raw double narrows with Wasm's saturating conversion. Checked
  `long` first compares the original double against the long range, and boxes
  only an out-of-range operand to produce the runtime's error. NaN becomes
  zero, and the rounded upper boundary, 2^63, becomes `Long/MAX_VALUE`, as in
  Clojure 1.12.6. Other values take the runtime cast's checks.
- **Return hints.** A `^long` or `^double` on an argument vector converts the
  completed body, including through the boxed entry. A tag on a `defn` name
  supplies its arities' default return kind; a vector's tag overrides it.
  Nested functions have their own tags. Return conversion happens outside the
  function's recurrence loop. A body already primitive in the source must
  match the declared kind (an int may widen to long, a float to double); an opposite primitive
  kind is a compile error. This check uses source hints, not WIT specialization
  or optimizer facts. Boxed bodies, including `try`, can unbox afterward.
  Clojure's primitive return unboxing differs from
  its parameter casts: a long return narrows a double with Java's saturating
  conversion (`1.0e30` becomes `Long/MAX_VALUE`), whereas a long parameter
  rejects an out-of-range double. Both reject a non-number. These results are
  still boxed at the generic entry's return.
  The source checker keeps byte, short, int and float array elements distinct
  from their physical i64/f64 representation. Unary and unchecked arithmetic
  follow the JVM overload's source result. Public bit-test/clear/flip/set stay
  boxed for source-kind checking, even when the optimizer keeps a bit-test
  result in a raw local. Array length is primitive int for a source-known array
  class, while an unhinted reflective call stays boxed. Immutable initializer
  facts are cached within an array-class query to bound shared-graph work.
- **Primitive bodies.** A function with source numeric parameter or return
  tags has a typed Wasm entry, retained in a signature-specific subtype of its
  code struct (ABI 7). The ordinary entry checks its arguments, calls that
  body and boxes a primitive result. Numeric operands and results inside the
  typed body stay raw; captures retain the ordinary closure representation.
  Variadic and spread adapters still reach the checked ordinary entry.
- **Fixed primitive calls.** Calls of a known local function, direct-linked
  definition or a var with numeric arglists can use the typed entry. Direct
  links retain their exact code-identity guard. Live var sites cache the typed
  reference alongside the ordinary entry, and refresh both when the callee
  changes. A typed hit keeps its numeric result raw. A guard miss can return
  any boxed replacement value, so the result keeps a raw slot, a boxed slot
  and a flag until a consumer needs it. Arithmetic and matching loop joins
  propagate those alternatives; numeric casts check the boxed alternative.
  The fallback never coerces a replacement merely because of an old hint.

Unhinted parameters, values read from vars, and ordinary call results are boxed.
Calls without a known primitive signature use the ordinary entry and box their
arguments. Functions that suspend (async continuations) keep every value boxed,
because their locals are saved as GC values across a suspension.

## Where values are boxed

A raw value is boxed at each use that needs a GC value: a call argument, a
collection element, a closure capture, a return, a `throw`, a loop local that
stays boxed, or any operation without a raw form. Within one block, a value is
boxed at most once, but each block boxes it again: a raw double that escapes
inside a loop allocates a box on every iteration, where the boxed path made one.
Results don't change, since `identical?` compares numbers by value
([scalar predicates](scalar-predicates.md)). A long boxes to an i31 when it
fits and to a `LONG` struct otherwise, and a double to a `NUMBER` struct; both
are inline, without a runtime call.

Long `+`, `-`, `*` and negation check for overflow inline. When the check
fails, the compiler calls the boxed runtime operation on boxed copies of the
operands, which throws "long overflow" exactly as the boxed path does.

## Not yet

These parts of #337, #448 and #351 aren't done yet:

- **Hinted `let` locals.** On the JVM, a hinted `let` local converts at each
  primitive use: `(let [^long x (identity 5.5)] [x (+ x 1)])` is `[5.5 6]`.
- **Additional typed call forms.** Variadic adapters, calls wider than the
  eight fixed entries, and async calls keep the ordinary interface. Primitive
  bodies themselves exist for all source-hinted methods. A live-var guard's
  fixed-entry shape excludes variadic bodies, whose final formal is a rest seq
  rather than a fixed argument; Wasm structural identity must distinguish them.
- **More raw consumers of guarded results.** Comparisons, identity tests and
  bitwise operations currently materialize guarded numeric results. Arithmetic,
  matching loop joins, primitive arguments and casts retain their raw hit.
- **Box reuse across blocks.** See "Where values are boxed" above (#351).

## Evidence

- `crates/suss-cli/tests/clojure_unboxed_locals.rs` computes each case twice:
  once on raw locals, and once on the same numbers passed through `identity`,
  which keeps them boxed. It covers overflow, NaN, -0.0, mixed longs and
  doubles, division, loops, closures and type errors, with JVM Clojure 1.12's
  results.
- `crates/suss-compile/tests/portable_unboxed_locals.rs` checks the compiled
  form of `(loop [i 0 acc 0] (if (< i 1234567) (recur (inc i) (+ acc i)) acc))`.
  Its only calls are the two overflow paths, and its only allocations box the
  operands for those paths or box the returned sum.
- `compiled_core_casts.rs` compares raw checked-double-to-long conversions
  with calls through core's var, including exact long boundaries and error
  messages. Its Runtime/Macro return tests and `compiled_primitive_params.rs`'s
  compiled components check return tags, multi-arity and named declarations,
  NaN, signed zero, conversion errors and recurrence against JVM observations.
  They also check primitive return mismatches and that WIT specialization of
  an unhinted source parameter does not create such a mismatch.
  Only a matching arglist vector's result tag makes a var invocation a source
  primitive result; a name-only default tag still converts the callee's body
  without imposing that source primitive kind on callers.
- `experiments/numeric-representation/suss-jvm/bench.py` times whole loops
  summing longs and doubles. Its README compares them with the unboxed `i64`
  kernel `d_i64_checked`.
- Its primitive sum loops compare exact main df4342dd with 3e825a18: the long
  loop drops from 308 to 108 instructions per iteration, and the double loop
  from 809 to 68; allocation drops from approximately 48 bytes to below the
  RSS measurement resolution in both. `(double i)` drops from 314 to 26
  instructions including the control, and approximately 16 bytes to below
  resolution. Code-generation and Runtime/Macro/component tests verify raw
  calls and coercions separately. Loaded-machine walltimes are not used for
  the program-ratio targets; commands and limitations are in the handoff.
