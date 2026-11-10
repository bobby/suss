# ADR-0004: Calling convention

Status: **Accepted** 2026-11-10.
Issue: [#359](https://github.com/bobby/suss/issues/359) (M15-05). Implemented
by [#353](https://github.com/bobby/suss/issues/353) (M15-06).

## Context

Every call in compiled Suss goes through one universal convention,
`(environment, argument-array) -> Value` ([portable closures](../runtime/portable-closures.md)).
A call to a one-argument function runs:

1. at the call site, a closure test turned into a boolean sentinel and tested
   again, and for anything else an inline protocol-key path (`callable-bind`);
2. `array.new_fixed` for the arguments;
3. the imported `invoke`, which checks the closure and array types and the
   arity range, unwraps the native-property wrapper around the environment
   (`closure-environment`), reads the closure's typed function reference, and
   `return_call_ref`s it;
4. the closure's dispatcher, which compares the array's length with each arity
   and tail-calls the method;
5. the method, which reads each argument with `array.get`.

On the M15-04 benchmark (`results/run_m15_04.txt`), a call to a user function
of 0–3 arguments costs 650–750 user-space instructions on a release build,
besides the loop. JVM Clojure's costs nothing measurable. M15-02 to M15-04 took
the var read, the predicates and the control flow out of that figure, so what
remains is the convention itself.

The convention also keeps ClojureScript's call semantics in one place: a native
protocol implementation called with too few operands receives internal
`undefined` for the rest, and surplus operands are dropped
([native protocols](../runtime/native-protocols.md)). Clojure, the oracle,
throws `ArityException` in both cases.

## Principle: use the platform as designed

Clojure gains from every JVM improvement without spending its core team's time,
because it compiles to what the JVM was built to run well. Suss takes the same
stance toward WebAssembly: closures and calls use WasmGC's own types and call
instructions in the way the proposals intend, and do not route around the
current costs of one engine. Where an engine is slow at something the design
relies on, that is recorded, measured and reported upstream
([`docs/upstream/`](../upstream/)), and Suss picks up the fix when the engine
ships it.

## Measurements

[`experiments/numeric-representation/src/calls.wat`](../../experiments/numeric-representation/src/calls.wat)
has one kernel per way of reaching a one-argument entry. Each loops doing
`acc = f(acc)` with an i31 increment as `f`, reading the callee from a mutable
global as a var's root is read. `calls.sh` runs them on the workspace's
wasmtime 49.0.1 and on V8 (Node 26.8.2), and counts user-space instructions
with `perf stat`, (count at n − count at 0) / n. The kernels:

| Kernel | How the call reaches its entry |
| --- | --- |
| `direct` | A static `call`, as direct linking (M15-07) emits |
| `indirect` | `call_indirect` through a `funcref` table, by an index read from the closure |
| `indirect_typed` | The same through a table typed as the signature |
| `table_ref` | `table.get` of that typed table, then `call_ref` |
| `field_typed` | `call_ref` of a typed function reference held in the closure itself |
| `field_untyped` | The same with a `funcref` field, cast to the signature |
| `index_final` | A closure test that branches out of line, the entry at table index `base + k`, `call_indirect`; one final closure type, with captures in a separate struct |
| `index_subtype` | The same with one closure subtype per function |
| `proposed` | This ADR: a closure test that branches out of line to a closure subtype, its function's code struct, the entry's typed reference, `call_ref` |
| `today` | The current path: callable test, argument array, `invoke`, dispatcher, method |

Instructions per call, less the inline increment
([raw run](../../experiments/numeric-representation/results/run_calls.txt)):

| Kernel | wasmtime 49 | V8, TurboFan without feedback |
| --- | --- | --- |
| `direct` | 9 | 0 (inlined) |
| `indirect` | 53 | 40 |
| `indirect_typed` | 50 | 38 |
| `table_ref` | 50 | 46 |
| `field_typed` | 205 | 32 |
| `field_untyped` | 173 | 37 |
| `index_final` | 82 | 57 |
| `index_subtype` | 172 | 57 |
| `proposed` | **328** | **49** |
| `today` | 402 | 90 |

On V8 with default tiering, TurboFan inlines monomorphic call sites from
Liftoff's feedback, and `proposed`, like the other kernels but `today`,
usually runs within about 1 ns of the inline increment; `today` runs 5–20 ns
more. These times vary with load and tier-up timing, so the instruction counts
are the comparison. The V8 counts are taken with
`--no-liftoff`, which compiles TurboFan code without that feedback, so they
show the cost at a polymorphic or cold site.

On V8, this ADR's call site is cheaper than any table-based one. On wasmtime
49, two of the platform's basic operations call into the host, both visible in
the compiled kernels with `objdump`:

- **Reading a function reference from a GC struct** calls
  `wasmtime_builtin_get_interned_func_ref`, typed or not: about 120–200
  instructions. Wasmtime stores a function reference in the GC heap as an ID
  into a side table ([upstream draft](../upstream/wasmtime-gc-funcref-reads.md)).
- **A cast to a non-final type** calls `wasmtime_builtin_is_subtype` when the
  value's type is a strict subtype: about 90 instructions
  ([upstream draft](../upstream/wasmtime-gc-subtype-casts.md)).

The table-index variants (`index_final`) avoid both and cost 82 instructions on
wasmtime 49, but they shape the ABI around one engine's present
implementation, and are 8 instructions slower on V8. Per the principle above,
this ADR doesn't adopt them.

## Decision

### Closures and code structs

A closure is a struct whose type is a subtype of the shared group's non-final
`$closure`:

| Field | Type | Holds |
| --- | --- | --- |
| `code` | `(ref $code)`, immutable | Its function's code struct |
| `properties` | `eqref`, mutable | Native protocol tables on this function, or null |
| `uid` | `eqref`, mutable | The lazily assigned identity UID, as now |

A function with captures defines its own final subtype of `$closure`, adding
one immutable field per capture; a function without captures uses `$closure`
itself. `properties` replaces the native-property wrapper around the
environment, so no call unwraps it.

Each function definition has one immutable code struct, shared by all its
closures, of the shared group's `$code` type:

| Field | Type | Holds |
| --- | --- | --- |
| `name` | `eqref` | Its display name, for error messages |
| `e0` … `e8` | `(ref $call0)` … `(ref $call8)` | The entry for k arguments |
| `spread` | `(ref $spread)` | The spread entry |

`$callk` is `(func (param (ref $closure) eqref × k) (result eqref))`: the
closure itself, then the arguments. `$spread` is
`(func (param (ref $closure) (ref $args) eqref) (result eqref))`: argument
values already realized, then the unrealized rest of the arguments, a seq or
nil. The spread entry serves calls with more than `MAX_FIXED` arguments,
`apply` and the host boundary.

`MAX_FIXED` is 8, which covers every fixed arity in core. A code struct is an
immutable global with a constant initializer (`struct.new` of `ref.func`s), so
a fragment needs no initialization code or host coordination to define one.
Its entries are:

- **A fixed arity `k ≤ MAX_FIXED`:** its method, which casts the closure to its
  function's final closure type to read captures.
- **An arity the function lacks:** the runtime's shared `arity-error-k`, which
  throws Clojure's message, `Wrong number of args (k) passed to: ns/name`, from
  `name`. No call site and no entry checks arity.
- **A variadic method with `r` required parameters:** for each `k` from `r` to
  `MAX_FIXED`, an adapter that packs the `k − r` surplus arguments as its rest
  seq, nil when there are none.
- **The spread entry:** it takes as many values from the array and the rest as
  the function's methods need. A fixed method needs no more than its arity, and
  the entry realizes one more to report wrong arity. A variadic method takes its
  rest as an unrealized seq, so `(apply f (range))` works for a variadic `f`, as
  in Clojure.

### Call sites

A call with `k ≤ MAX_FIXED` arguments, to a callee that isn't known statically:

```wat
local.get $f
br_on_cast_fail $slow eqref (ref $closure)  ;; nil, unbound and non-closures leave
local.tee $c
<arguments>
local.get $c  struct.get $closure $code  struct.get $code $ek
call_ref $callk                             ;; return_call_ref in tail position
...
$slow:                                      ;; out of line, at the function's end
<arguments>
call $call-k                                ;; runtime import
```

With more than `MAX_FIXED` arguments, the site builds the argument array, with
nil for the rest, and calls the spread entry the same way. The site has no
argument array, no arity check and no callable-protocol path.

The runtime's `call-k` and `call-spread` take the callee and the arguments.
They handle every callable that isn't a closure:

- keywords, maps, sets and vectors, as Clojure's `IFn`;
- a var, which derefs and calls;
- a user type with an `IFn` implementation, through its descriptor;
- an unbound var's marker, which throws Clojure's unbound-fn error;
- anything else, which throws Clojure's `cannot be cast to clojure.lang.IFn`.

This is #313: the uncommon paths of every call move into one shared helper per
arity, so a call site shrinks from 280–490 bytes to about 30.

Where the compiler knows the callee, it calls the entry directly: a named `fn`
calling itself, a local `fn` bound by `let` and never rebound, and, under
M15-07, a direct-linked var. That is `call $entry-k` with the closure as the
first argument, 9 instructions on wasmtime.

### `apply` and the host boundary

`apply` realizes at most `MAX_FIXED + 1` items of its last argument. If they
are all there are, it calls entry `k`; otherwise it calls the spread entry with
the realized values and the remaining seq. Host calls into Suss callables, such
as WIT exports that call a var's function and callbacks passed out as
resources, go through `call-spread`.

### No ClojureScript call semantics

Native protocol implementations are closures like any other, so calling one
with the wrong number of operands reaches an `arity-error-k` entry. Nothing on
the call path fills missing parameters with `undefined` or drops surplus
operands. `undefined` keeps one role: the internal unbound marker that a var
read yields for an unbound cell and that `call-k` reports. It never reaches
source code as a value.

### Primitive entry points (#337)

#337 adds them on top of M15-06, as the M15 plan has it. A function with
`^long` or `^double` parameters or result gets a code struct of a subtype of
`$code` for its primitive signature, adding a typed entry such as
`(func (param (ref $closure) i64) (result i64))` for `[^long x] ^long`. Only
call sites that know the callee's signature use it: direct-linked calls
(M15-07) and calls through a var whose `:arglists` carry the hints, as
Clojure's compiler does with `IFn$LL`. Since a var can be redefined without the
hints, such a site casts the code struct to the subtype with `br_on_cast` and
falls back to the boxed entry. M15-06 reserves nothing for them.

### ABI version and migration

This is runtime ABI version 4:

- The shared group's `$closure` and `$code` types, and the `$callk`, `$spread`
  and `$args` types, replace today's closure struct and invoke type.
- The runtime exports `call-0` to `call-8`, `call-spread`, `arity-error-0` to
  `arity-error-8` and `apply`; `invoke` and `closure-environment` go.
- Compiled artifacts record `:runtime-abi 4`. An artifact compiled for ABI 3
  fails early with the existing mismatch error and must be recompiled. M15-06
  regenerates the bootstrap.

## Consequences

- **Cost.** A direct-linked or otherwise known call costs 9 instructions on
  wasmtime, so the M15 target for direct-linked calls (under 50) falls to
  M15-07. A call through an unknown closure costs 49 instructions on V8 at a
  cold or polymorphic site, and inlines at a monomorphic one. On wasmtime 49 it
  costs 328, against 402 today, until the two host calls above become inline
  code. Higher-order code (`reduce`, `map`, transducers, protocol dispatch, var
  calls in the REPL) pays that on wasmtime, which puts the M15 collection
  targets partly in the engine's hands; M15-06 measures it in Suss.
- **Engine improvements apply automatically.** Faster function-reference reads
  or subtype casts in wasmtime, or better inlining in either engine, speed up
  every Suss call without a change to Suss.
- **Size.** A call site shrinks from 280–490 bytes to about 30. Each function
  adds one code struct of 11 references, shared by its closures; a closure
  holds only its code, properties, UID and captures, smaller than today's.
- **Semantics.** Wrong arity throws Clojure's `ArityException` message for
  every callable kind, including native protocol implementations. Tests compare
  with JVM Clojure 1.12.6 (#353).
- **Complexity.** A function becomes up to `MAX_FIXED + 2` entries plus
  adapters, and the variadic and spread entries carry the seq logic that the
  dispatcher has now. The compiler, not the runtime, owns arity.

## Alternatives considered

- **Table indices and `call_indirect`** (`index_final`). The closure holds the
  table index of its entries, in one shared `funcref` table that fragments grow
  and fill. Four times cheaper than this ADR on wasmtime 49, and slightly
  dearer on V8. Rejected because it exists only to avoid wasmtime 49's
  function-reference reads, and it costs a table that every fragment must
  grow, fill and keep consistent.
- **One final closure type with captures in a separate struct**, avoiding
  wasmtime 49's subtype-cast host call. Rejected for the same reason: it costs
  every capturing closure a second allocation and every capture read a second
  load, to suit one engine.
- **Entries held in each closure** (`field_typed`), as #353 first proposed.
  Every closure allocation would copy `MAX_FIXED + 2` references; a code struct
  shared per function costs one load more.
- **Keep the universal convention and only inline its fast path.** The
  argument array, the dispatcher and the function-reference read remain; the
  `today` kernel already omits the environment unwrap and costs 402.
- **Clojure's 20 fixed arities.** Each fixed arity adds a field to every code
  struct and a `call-k` and `arity-error-k` to the runtime. In core, the test
  suite and the oracle corpus, 97–98% of call sites pass at most 4 arguments,
  and no fixed parameter list in core has more than 8. Wider calls are almost
  all to variadic functions, where a wider entry would still build a rest seq,
  or to core-internal helpers that direct linking calls statically.

## Resolved questions

1. **`MAX_FIXED`:** 8, decided 2026-11-10, against 4 or Clojure's 20. Calls
   with more arguments go through the spread entry. Clojure's compile-time
   limit of 20 fixed parameters is a separate language rule, which Suss keeps.
2. **Primitive entries:** with #337, decided 2026-11-10, as the M15 plan has
   it.
3. **Engine-specific layouts:** not adopted, decided 2026-11-10. Wasmtime's
   costs are tracked in [`docs/upstream/`](../upstream/).

## References

- [#349](https://github.com/bobby/suss/issues/349), the M15 tracking issue and profile.
- [#353](https://github.com/bobby/suss/issues/353), the implementation.
- [#313](https://github.com/bobby/suss/issues/313), out-of-line call paths.
- [#337](https://github.com/bobby/suss/issues/337), primitive entry points.
- Clojure's `clojure.lang.IFn`, `AFn.applyToHelper` and `RestFn`.
