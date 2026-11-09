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
  and `dec` are `+` and `-` of 1. Shipped core's coercing arithmetic stays
  boxed.
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

A function's parameters, values read from vars, and results of calls are
boxed. Functions that suspend (async continuations) keep every value boxed,
because their locals are saved as GC values across a suspension.

## Where values are boxed

A raw value is boxed at each use that needs a GC value: a call argument, a
collection element, a closure capture, a return, a `throw`, a loop local that
stays boxed, or any operation without a raw form. Within one block, a value is
boxed at most once. A long boxes to an i31 when it fits and to a `LONG`
struct otherwise, and a double to a `NUMBER` struct; both are inline, without a
runtime call.

Long `+`, `-`, `*` and negation check for overflow inline. When the check
fails, the compiler calls the boxed runtime operation on boxed copies of the
operands, which throws "long overflow" exactly as the boxed path does.

## Not yet

These parts of #337 aren't done yet:

- **Hints.** Clojure's `^long` and `^double` on parameters and `let` locals.
  On the JVM, a hinted `let` local converts at each primitive use:
  `(let [^long x (identity 5.5)] [x (+ x 1)])` is `[5.5 6]`. A hinted
  parameter converts on entry: `((fn [^long n] n) 1.5)` is 1.
- **Known core results.** Calls such as `(long x)` and `(double x)` aren't yet
  treated as raw.
- **Primitive entry points.** These wait for the calling convention in #353.

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
- `experiments/numeric-representation/suss-jvm/bench.py` times whole loops
  summing longs and doubles. Its README compares them with the unboxed `i64`
  kernel `d_i64_checked`.
