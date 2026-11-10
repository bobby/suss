# ADR-0004: Calling convention

Status: **Proposed**, with [questions for review](#questions-for-review).
Date: 2026-11-10.
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
| `field_typed` | `call_ref` of a typed function reference read from the closure struct |
| `field_untyped` | The same with a `funcref` field, cast to the signature |
| `proposed` | This ADR: a closure test that branches out of line, the entry at `base + k`, `call_indirect`, and the callee reading a capture from its environment |
| `proposed_subtype` | The same with one closure subtype per function instead of an environment |
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
| `proposed` | **82** | **57** |
| `proposed_subtype` | 172 | 57 |
| `today` | 401 | 90 |

On V8 with default tiering, TurboFan inlines monomorphic call sites from
Liftoff's feedback, and every kernel but `today` runs within 0.7 ns of the
inline increment (`today`: 4.8 ns more). The V8 counts are taken with
`--no-liftoff`, which compiles TurboFan code without that feedback, so they
show the cost at a polymorphic or cold site.

Three findings decide the design:

- **Wasmtime can't keep code pointers in the GC heap.** A function reference
  stored in a GC struct, typed or not, is an interned ID, and reading it calls
  the host (`wasmtime_builtin_get_interned_func_ref`, visible in the compiled
  kernel). That is 173–205 instructions per call: the call through today's typed
  `invoke` field pays it. V8 reads the same field in a few instructions.
- **A table index costs 50 instructions on wasmtime and 38–46 on V8.** The
  `call_indirect` is all inline: the table bounds check, wasmtime's lazy
  initialization test (`Config::table_lazy_init(false)` removes it), the
  signature check and the call. A table typed as the signature saves only the
  signature check, 3 instructions.
- **On wasmtime 49, a cast to a non-final type calls the host** whenever the
  value's type is a strict subtype (`wasmtime_builtin_is_subtype`): 90
  instructions in `proposed_subtype`. A cast to a final type is an inline
  type-ID comparison. So the closure type stays final, and per-function data
  hangs off it in a field, whose final type the callee casts to.

The kernels run within one module. Suss's fragments call through a table that
the runtime instance owns, which adds a load or two for the imported table;
M15-06 measures that in Suss itself.

## Decision

### Closures and entries

A closure is a struct of one final type in the shared group:

| Field | Type | Holds |
| --- | --- | --- |
| `base` | `i32`, immutable | The table index of the function's 0-argument entry |
| `env` | `eqref`, immutable | Its captures: a final struct type defined by its fragment, or null |
| `name` | `eqref`, immutable | Its display name, for error messages |
| `properties` | `eqref`, mutable | Native protocol tables on this function, or null |
| `uid` | `eqref`, mutable | The lazily assigned identity UID, as now |

`properties` replaces the native-property wrapper around the environment, so
no call unwraps it.

Every function has `MAX_FIXED + 2` consecutive slots in the shared function
table, from `base`:

- **`base + k`, for `k` from 0 to `MAX_FIXED`:** the entry for `k` arguments,
  of the shared type `$callk = (func (param (ref $closure) eqref × k) (result eqref))`.
  The closure itself is the first parameter.
- **`base + MAX_FIXED + 1`:** the spread entry,
  `(func (param (ref $closure) (ref $args) eqref) (result eqref))`: argument
  values already realized, then the unrealized rest of the arguments, a seq or
  nil. It serves calls with more than `MAX_FIXED` arguments, `apply` and the
  host boundary.

`MAX_FIXED` is 4 (see [questions](#questions-for-review)). Entries are
generated per function:

- **A fixed arity `k ≤ MAX_FIXED`:** its method, with its captures read from
  `env`, which the method casts to its final environment type.
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

### The function table

The runtime instance owns one `funcref` table and exports it, and every
fragment imports it. A fragment's initializer reserves its slots with
`table.grow`, keeps the returned base in a mutable global, and fills them with
`table.init` from a passive element segment. That segment refers to the
fragment's own functions and to the imported `arity-error-k` functions.
Closure construction adds the function's offset to that base. No host
coordination is needed, and the same code works in the persistent REPL, in AOT
components (whose core instances pass the table between them) and in browsers.
The table only grows: a fragment stays loaded for the session's life, as now.

### Call sites

A call with `k ≤ MAX_FIXED` arguments, to a callee that isn't known statically:

```wat
local.get $f
br_on_cast_fail $slow eqref (ref $closure)  ;; nil, unbound and non-closures leave
local.tee $c
<arguments>
local.get $c  struct.get $closure $base  i32.const k  i32.add
call_indirect $functions (type $callk)      ;; return_call_indirect in tail position
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

Where the compiler knows the callee, it calls the entry directly, with no
table: a named `fn` calling itself, a local `fn` bound by `let` and never
rebound, and, under M15-07, a direct-linked var. That is `call $entry-k` with
the closure as the first argument, 9 instructions on wasmtime.

### `apply` and the host boundary

`apply` realizes at most `MAX_FIXED + 1` items of its last argument. If they
are all there are, it calls `base + k`; otherwise it calls the spread entry
with the realized values and the remaining seq. Host calls into Suss callables,
such as WIT exports that call a var's function and callbacks passed out as
resources, go through `call-spread`.

### No ClojureScript call semantics

Native protocol implementations are closures like any other, so calling one
with the wrong number of operands reaches an `arity-error-k` entry. Nothing on
the call path fills missing parameters with `undefined` or drops surplus
operands. `undefined` keeps one role: the internal unbound marker that a var
read yields for an unbound cell and that `call-k` reports. It never reaches
source code as a value.

### Primitive entry points (#337)

A function with `^long` or `^double` parameters or result also gets primitive
entries, after its boxed slots: `base + MAX_FIXED + 2 + k` holds a function of
the shared type for its primitive signature, for example
`(func (param (ref $closure) i64) (result i64))` for `[^long x] ^long`, or a
shared boxing adapter. Only call sites that know the callee's signature use
them: direct-linked calls (M15-07) and calls through a var whose `:arglists`
carry the hints, as Clojure's compiler does with `IFn$LL`. Since a var can be
redefined without the hints, such a site tests the slot's type
(`table.get`, `ref.test`) and falls back to the boxed entry. #337 implements
this on top of M15-06, as planned.

### ABI version and migration

This is runtime ABI version 4:

- The shared group's closure struct changes as above, and the `$callk` and
  spread entry types join it. The invoke type goes.
- The runtime exports `functions`, `call-0` to `call-4`, `call-spread`,
  `arity-error-0` to `arity-error-4` and `apply`; `invoke` and
  `closure-environment` go.
- Compiled artifacts record `:runtime-abi 4`. An artifact compiled for ABI 3
  fails early with the existing mismatch error and must be recompiled. M15-06
  regenerates the bootstrap.

## Consequences

- **Cost.** A call that isn't direct-linked costs about 82 instructions on
  wasmtime in the kernel, against 401 for today's path, and about 8 ns under
  load. A direct-linked call costs 9. On V8, every call outside polymorphic
  sites inlines. The M15 target for direct-linked calls (under 50
  instructions) falls to M15-07; this convention is what makes it reachable.
- **Size.** A call site shrinks from 280–490 bytes to about 30, and every
  function adds six table slots (48 bytes of table per instance) and up to five
  entries, most of them shared error stubs.
- **Semantics.** Wrong arity throws Clojure's `ArityException` message for
  every callable kind, including native protocol implementations. Tests compare
  with JVM Clojure 1.12.6 (#353).
- **Complexity.** A function becomes up to `MAX_FIXED + 2` entries plus
  adapters, and the variadic and spread entries carry the seq logic that the
  dispatcher has now. The compiler, not the runtime, owns arity.
- **Engines.** The design avoids wasmtime's two host calls (function references
  in GC structs, casts to non-final types) and uses only `call_indirect`, which
  every engine optimizes. If a later wasmtime keeps function references in the
  GC heap cheaply, typed fields could replace the table without changing the
  entries.

## Alternatives considered

- **Typed function references in the closure** (`field_typed`), as #353
  proposed. Cheapest on V8 by a few instructions, but a host call per call on
  wasmtime, which is the WASI target.
- **One closure subtype per function** (`proposed_subtype`), so captures are
  fields. Saves the environment load, but the call site's cast to the supertype
  is a host call on wasmtime 49.
- **A table of typed references and `call_ref`** (`table_ref`). No faster
  than `call_indirect`, and the typed table can't hold the entries of different
  arities.
- **Keep the universal convention and only inline its fast path.** The
  argument array, the dispatcher and the function-reference read remain; the
  `today` kernel already omits the environment unwrap and costs 401.
- **Clojure's 20 fixed arities.** See the questions.

## Questions for review

1. **`MAX_FIXED`: 4 or 20?** Clojure's `IFn` has 20 fixed arities. Each extra
   one costs every function a table slot, an element-segment entry and, unless
   the function has that arity, nothing else, since the error stubs are shared.
   Calls with more arguments than `MAX_FIXED` still work, through the spread
   entry, at about today's cost. 4 covers the calls #349 measured and almost all
   core functions. Recommended: 4, raised if `bench.py` or the corpus shows
   common wider calls.
2. **Primitive entries in M15-06 or with #337?** Recommended: #337, as the M15
   plan has it; M15-06 reserves no slots for them, since they follow the boxed
   ones.

## References

- [#349](https://github.com/bobby/suss/issues/349), the M15 tracking issue and profile.
- [#353](https://github.com/bobby/suss/issues/353), the implementation.
- [#313](https://github.com/bobby/suss/issues/313), out-of-line call paths.
- [#337](https://github.com/bobby/suss/issues/337), primitive entry points.
- Clojure's `clojure.lang.IFn`, `AFn.applyToHelper` and `RestFn`.
