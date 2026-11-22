# Unboxed numeric locals

Inside a compiled function, the compiler keeps a long as a raw `i64` Wasm
local and a double as a raw `f64`, and boxes a number only where it escapes
([ADR-0003](../adr/0003-clojure-numeric-tower.md) "Representation", #337).
Storage optimization preserves each selected source operation's values,
overflow checks and exceptions. Clojure's generic Object division and primitive
double division are distinct source overloads even when both operands are held
as boxed doubles.

## What stays raw

After lowering to IR, `emit/unboxed.rs` infers each value's representation:

- **Literals.** A long or double literal that feeds raw arithmetic, a raw
  comparison or a raw loop local.
- **Arithmetic.** `+`, `-` and `*` of longs give a raw long; any double operand
  gives a raw double. `/` can give a raw double when its source overload is
  primitive and an operand is a double. Generic division keeps its checked
  runtime route, preserving NaN propagation and the boxed zero check. A
  quotient of two longs stays boxed, since only an exact one is a long. `inc`
  and `dec` are `+` and `-` of 1. Core's coercing `+`, `-` and `*` also keep
  two known numeric operands raw, matching the helpers' checked numeric route.
  Guarded primitive results keep their numeric hit raw and their original
  coercing helper on a boxed fallback. Unknown operands, unary coercions and
  core's division retain their original boxed path.
- **Other numeric core results.** Direct `quot`, `rem`, `abs` and unchecked
  arithmetic calls use numeric source inlining even without local hints. Known
  long/double inputs and results stay raw. Unchecked longs wrap; long quotient
  handles MIN/-1 without a Wasm trap. Quotient/remainder retain the boxed zero
  and non-finite error paths, and double quotient truncates through a long only
  within its finite range. Remainder uses the original quotient/subtraction
  formula, including negative zero and an infinite divisor. Unknown operands
  retain checked runtime helpers. Direct calls ignore var replacements as
  Clojure's inline forms do; ordinary aliases still invoke the replacement.
  Guarded primitive operands keep a raw hit and an original boxed miss, so a
  replacement number retains its actual kind and sign.
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
  A guarded primitive call's numeric hit can also compare raw. If either
  input instead holds a boxed replacement result, the comparison uses its
  original runtime helper, including its coercions or errors.

- **Bitwise operations.** `bit-and`, `bit-or` and `bit-xor` of two longs give a
  raw long (#387). `bit-test` of two raw longs gives a raw truth value; the
  shift count uses its low six bits, including for negative counts, as Java's
  long shift does. The truth value is boxed where it escapes. Other operand
  representations take the runtime operation, which throws for a double as
  Clojure does.
  Core's private 32-bit operations also stay raw for long operands, using their
  low 32 bits, five-bit shift counts and the original signed result extension
  (unsigned for zero-fill right shift). Unknown operands keep their existing
  ToInt32 conversion path. Those helpers also guarantee a long result: the
  compiler extracts it once for subsequent raw uses and caches the existing
  GC value for uses that need it boxed. Literals used only by the boxed path
  keep their existing constant representation.
  Guarded long inputs also take the raw path when every input hits its
  primitive entry. On a miss, the original int32 helper converts the boxed
  replacement values and supplies the guaranteed long result.
- **Extrema.** Direct `max`/`min` calls nest binary source overloads. Long/long
  results stay raw with an integer comparison/select; double/double uses raw
  IEEE min/max, including signed zero. Mixed overloads preserve the selected
  argument type and stay boxed when that type is unknown. Generic Object calls
  propagate NaN and select the second argument on ties, even when optimization
  holds both values as raw doubles. Local hints convert at selected primitive
  formals, before later operands execute. Aliases retain ordinary function-call
  argument evaluation and generic Object semantics. Guarded double inputs keep
  their raw hit; a lost signature preserves the original boxed replacement
  without coercing a long to double merely because of the old declaration.
  Boxed captures use Math only for genuine doubles. The source classifier keeps
  primitive return/initializer diagnostics independent of physical storage.
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
  rebinds it. A source primitive parameter requires a matching primitive
  `recur` operand (int may widen to long and float to double). A hinted Object
  alone cannot supply that operand; ordinary loop locals can still box on a
  mismatched recurrence. A WIT export's numeric parameters use the same storage conversion
  ([WIT numbers](wit-numbers.md), "Primitive export parameters").
- **Hinted locals.** A `^long` or `^double` let/loop local retains its
  original Object value. Its hint supplies a numeric class at primitive uses:
  `(let [^long x (identity 5.5)] [x (+ x 1)])` returns `[5.5 6]`.
  Covered arithmetic, numeric comparisons/predicates, quot/rem/abs, unchecked
  increment/decrement and numeric casts consume these hints at primitive uses;
  ordinary function calls retain the Object. Binary `=` consumes a numeric hint
  for the selected long/long, double/double or primitive/Object overload. Mixed
  long/double `=` and floating-class bitwise calls keep the original Objects,
  matching Clojure's reflective calls. Same-category primitive equality compares
  raw values. Some overloaded families still need the selection described below. Aliases and captures keep the same source
  class. Nary comparisons are ordinary calls and ignore local hints. A use-site
  hint overrides an Object local's declaration; a contradictory hint on a
  genuine primitive local is an error. A hinted binding with a primitive
  initializer is also an error, diagnosed before loop boxing/reanalysis.
  Source classes and primitive expression kinds are independent of the
  backend's raw storage inference.
- **Core coercions.** `long`, `double`, `unchecked-long` and
  `unchecked-double` produce raw results. A raw long stays raw or widens to
  a double; a raw double narrows with Wasm's saturating conversion. Checked
  `long` first compares the original double against the long range, and boxes
  only an out-of-range operand to produce the runtime's error. NaN becomes
  zero, and the rounded upper boundary, 2^63, becomes `Long/MAX_VALUE`, as in
  Clojure 1.12.6. Other values take the runtime cast's checks.
  Narrow integer/float conversions also keep their known numeric results raw.
  Raw-return helpers accept boxed operands without a temporary numeric result
  box, while the original boxed entries remain available. Checked float(long)
  has a separate source-selected mode to preserve its single rounding;
  float(Object) retains the double intermediate. Source float(float) permits
  infinities. Object byte/short/int casts narrow the truncated Number.longValue(),
  while genuine double casts compare the original value. Byte/short errors still
  print the original Object. Source int(float) compares against the rounded 2^31
  upper bound and saturates to Integer/MAX_VALUE there. These modes are selected
  before consuming a local hint and independently of raw storage.
- **Return hints.** A `^long` or `^double` on an argument vector converts the
  completed body, including through the boxed entry. A tag on a `defn` name
  supplies its arities' default return kind; a vector's tag overrides it.
  Nested functions have their own tags. Return conversion happens outside the
  function's recurrence loop. A body already primitive in the source must
  match the declared kind (an int may widen to long, a float to double); an opposite primitive
  kind is a compile error. This check uses source hints, not WIT specialization
  or optimizer facts, and includes lexical captures and enclosing primitive
  parameters. Checks run in the initial loop context before boxing/reanalysis;
  boxed bodies, including `try`, can unbox afterward.
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
  typed body stay raw. Materialized closures can keep numeric-only captures in
  raw fields as described below.
  Variadic and spread adapters still reach the checked ordinary entry.
- **Fixed primitive calls.** Calls of a known local function, direct-linked
  definition or a var with numeric arglists can use the typed entry. Direct
  links retain their exact code-identity guard. Live var sites cache the typed
  reference alongside the ordinary entry, and refresh both when the callee
  changes. A typed hit keeps its numeric result raw. A guard miss can return
  any boxed replacement value, so the result keeps a raw slot, a boxed slot
  and a flag until a consumer needs it. Arithmetic, comparisons and matching loop joins
  propagate those alternatives; numeric casts check the boxed alternative.
  The fallback never coerces a replacement merely because of an old hint.


- **Numeric capture fields.** A final closure subtype can use immutable i64/f64
  fields for captures whose complete uses in every method are inline numeric
  operations. Entry inference receives the field representation without changing
  the source class or parameter types. Literal captures count as raw uses at
  their maker. Returning, throwing, identity, nested capture and unknown uses
  retain shared object fields, including through CFG aliases. Guarded numeric
  alternatives and resumable makers stay conservative. Named, multiple-arity
  and variadic closures use the same layout decision; the closure itself can
  still allocate. See `results/m12-numeric-captures` for source and observations.

- **Recursive local groups.** letfn/letfn* declare all function names before
  analyzing initializers. A private object array is filled with their closures
  before the body runs. Captures refer to a group/slot for forward calls, including
  through nested functions and suspended futures. The source binding remains a
  function; its runtime group is not a new source primitive class. Numeric-only
  captures use the same raw-field analysis. Group arrays and recursive closure
  objects still allocate. See `results/m12-letfn` for JVM, GC, async and source
  snapshot observations.

- **Locally called closures.** After source overload/return/recur checks, a
  fixed-arity closure whose complete use set is correctly sized calls can be
  removed. Its body moves into each call site; captures and evaluated arguments
  flow through CFG parameters, retaining declared IR types while inference keeps
  numeric storage raw. Always-throwing bodies remove their unreachable suffix and
  unused SSA records. One-use bodies move; repeated sites share a bounded graph
  copying allowance. Returned/stored/unknown-function closures, general/variadic
  bodies, CFG-carried closure values and normalized continuations remain
  conservative. This does not infer new source primitive types for call results.

Unhinted parameters, values read from vars, and ordinary call results are boxed.
Calls without a known primitive signature use the ordinary entry and box their
arguments. Functions that suspend (async continuations) keep every value boxed,
because their locals are saved as GC values across a suspension.

## Where values are boxed

A raw value needs a GC copy for a call argument, collection element, closure
capture, return, throw, boxed loop local, or an operation without a raw form.
A copy is created lazily. A value used at multiple materialization sites, or in
an escapable CFG cycle without reexecuting its definition, has a dynamic cache
flag and reuses that copy across blocks. Other values retain the simple per-block
copy. Entering a defining block invalidates its result and parameter caches;
values defined outside the cycle retain one copy. No allocation moves before
its first use. Inline arithmetic's throwing paths and a single return do not
by themselves add cache flags to the hot loop.
Results don't change, since `identical?` compares numbers by value
([scalar predicates](scalar-predicates.md)). A long boxes to an i31 when it
fits and to a `LONG` struct otherwise, and a double to a `NUMBER` struct; both
are inline, without a runtime call.

Long `+`, `-`, `*` and negation check for overflow inline. When the check
fails, the compiler calls the boxed runtime operation on boxed copies of the
operands, which throws "long overflow" exactly as the boxed path does.

## Other paths and limits

Some source and calling paths retain the following behavior:

- **Source types and WIT.** A local hint supplies a class; the selected formal
  parameter must be primitive before lowering consumes it. WIT-only parameter
  conversions keep their checked numeric storage but do not invent source
  primitive types for return, capture or recur diagnostics. Historical overload
  negatives retain their original source identities; the extrema and primitive
  method receipts record subsequent corrections within their stated scope.
- **Additional typed call forms.** Variadic adapters, calls wider than the
  eight fixed entries, and async calls keep the ordinary interface. Primitive
  bodies themselves exist for all source-hinted methods. A live-var guard's
  fixed-entry shape excludes variadic bodies, whose final formal is a rest seq
  rather than a fixed argument; Wasm structural identity must distinguish them.
- **More raw consumers of guarded results.** Most comparisons, identity tests
  and bitwise operations materialize guarded numeric results. Double extrema,
  arithmetic, numeric core operations, matching loop joins, primitive arguments and casts retain their
  raw hit and preserve the original boxed value on a signature miss.
- **Box reuse across blocks.** See "Where values are boxed" above (#351).

## Evidence

- `clojure_nonescaping_closures.rs` compares30 fresh JVM values in Runtime,
  Macro and actual AOT, including nested/escaping closures, arity, primitive casts,
  mixed results, NaN, signed zero, overflow and effect/exception order. The new
  exact inspected loop executes1,234,591 iterations and returns3705297213702690.0
  as on JVM, with no normal-path calls or loop-carried boxes. Four benchmark
  cases retain actual before/after instruction and null-GC RSS samples: the local
  closure falls from about120 estimated bytes/1777 instructions per iteration to
  below RSS resolution/about28 instructions, matching the direct control.
  Temporary vectors/MapEntries remain about168/136 bytes. See
  `results/m12-escape`; these are allocation/instruction results, not quiet times.
- `clojure_raw_bits_and_casts.rs` executes48 fresh JVM value assertions and4
  separately labeled Suss redefinition controls in both phases and actual AOT.
  `results/m12-cast-overloads` extends the original30 cases with Object/float
  boundaries, NaN, errors, ordinary aliases, hints and evaluation order. The
  original six mismatches and two controls remain in the frozen
  `results/m12-bits-casts` receipts; the new regressions cover their corrections.
  Raw helper artifact checks require exact scalar result types and reject
  numeric box construction.
- `crates/suss-cli/tests/clojure_raw_numeric_core.rs` matches22 fresh JVM
  value assertions in Runtime/Macro and actual AOT, plus4 separately labeled
  Suss redefinition controls matching the frozen before compiler. The two new
  `portable_unboxed_locals.rs` loops execute exactly the inspected components:
  known long/double core operations have no normal-path helper calls or
  loop-carried allocations. Both loop results also match fresh JVM execution.
  `results/m12-raw-core` retains source reconstruction, failed-before controls,
  complete gate logs and explicit limits.
- `crates/suss-cli/tests/clojure_primitive_method_types.rs` compares captured
  returns and primitive recurrence with fresh JVM Clojure1.12.6:17 values and15
  diagnostics, in Runtime/Macro and actual AOT. It includes reflective bitwise
  returns, named arities, primitive parameter captures, widening and loop boxing.
  `compiled_primitive_params.rs` also executes WIT-only capture/recur controls.
  Source patches, hashes, failing-before controls and commands are retained in
  `results/m12-primitive-methods`.
- `crates/suss-cli/tests/clojure_unboxed_locals.rs` computes each case twice:
  once on raw locals, and once on the same numbers passed through `identity`,
  which keeps them boxed. It covers overflow, NaN, -0.0, mixed longs and
  doubles, loops, closures and type errors, with JVM Clojure 1.12's results.
  Division zero cases explicitly distinguish generic source overloads from
  primitive ones; the expanded overload matrix also executes actual AOT
  components in `clojure_division_overloads.rs`.
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


### Literal vector storage

An eligible non-escaping literal vector can retain its evaluated elements as
scalars after source typing. Constant in-bounds indexed reads feed parameters
with their original generic IR types, so physical numeric storage does not
invent primitive source casts. A current-constructor/undefined guard keeps the
original construction and lookup continuation available. Successful immutable
schema admission is cached per fragment site; current canonical registration is
still checked on every invocation. The bounded pass handles one eligible
1–31-element vector per function and leaves unknown indices, escapes and
normalized async/future plans conservative. Source, failing probes, execution and
allocation/instruction receipts are in [vector evidence](../../results/m12-vectors/README.md).
The retained aliased-NaN difference is not a JVM compatibility success.


For the genuine shipped `nth`, its indexed miss also uses the owned core
function, as Clojure's inline expansion does under root rebinding. Lists,
strings and missing/default indices therefore keep the same inline behavior as
native hits. The source AST preserves the public var; lexical/unknown calls and
aliases remain ordinary calls. [Inline nth evidence](../../results/m12-vector-nth-oracle/README.md)
keeps the original pinned ClojureScript traces and explicit JVM corrections.


The same guarded continuation also handles a non-escaping `(MapEntry. k v nil)`
with constant indexed projections. Key/value/hash effects remain ordered and
its public construction guard can disappear only after canonical MapEntry
admission. Core appends one private descriptor registry without moving existing
global/type indices. The [MapEntry receipts](../../results/m12-mapentries/README.md)
retain source reconstruction, execution and allocation/instruction evidence.
