# Shared GC runtime ABI v1

`suss_compile::runtime_abi` emits the new core runtime with wasm-encoder 0.258.0.
This is production runtime implementation, not a second source compiler or the
M0 hand-written feasibility fixture. The legacy compiler has **not** migrated;
the source differential corpus still reports 9 passing and 7 known failures.
The [portable compiler bootstrap](portable-pipeline.md) now executes float and
UTF-16 source fragments on this ABI. M2-03 remains incomplete: general closure/
callee lowering, dynamic checks, production loaders and corpus migration remain.

Every runtime/fragment begins with the identical explicit recursive group from
`prelude()`. Additional function types follow it. Construction indices are
internal; artifact compatibility uses versions and the actual complete prelude
layout, not exported type-index identifiers. The layouts are:

| Type | Fields/storage |
| --- | --- |
| Number | Immutable f64; all ordinary numbers are boxed |
| UTF-16 | Packed u16 array; mutable construction storage |
| Arguments | Value array, initialized with the nil sentinel |
| Invoke | `(environment: Value, arguments: Arguments) -> Value` |
| Closure | Environment, typed invoke reference, minimum/maximum arity; maximum -1 means variadic |
| Binding cell | Mutable value and bound flag, independent of nil |
| Descriptor | Nominal identity, field schema, mutable protocol table and metadata |
| User object | Descriptor reference, fields, object metadata |
| Exception | Descriptor reference, UTF-16 message, data, cause |
| Dynamic frame | Parent and binding entries |

Value is eqref. Nil/false/true use i31 sentinels 0/2/4; ordinary integers do not
use i31 tags. GC roots the runtime's built-in error descriptor once per instance.
The descriptor/protocol/object/frame layouts are established, but their complete
language machinery remains M2-04/M3 work.

Implemented intrinsics box/unbox binary64, add/subtract/multiply/divide/negate,
allocate/read UTF-16 units, reject invalid unit writes without truncation, create
argument arrays and closures, invoke with central arity checks, and create/get/set
initialized binding cells. Wrong arity throws a typed language exception with
`Wrong arity` diagnostic; independently loaded callers can catch its exported
tag. Storage/math intrinsics are for verified lowering, not public core APIs.
Unchecked allocation/type/index operations require compiler guards before user
inputs reach them; complete dynamic type diagnostics remain integration work.
String mutation is construction-only; source-level immutable string semantics
must be enforced by lowering. Binding allocation currently creates initialized
cells; declaration/unbound-var behavior remains M3 integration work.

`Manifest::section()` records runtime ABI version, compiler package version and
the pinned wasm-tools family version. `verify_artifact()` rejects absent,
duplicate, malformed or incompatible manifests and checks the actual explicit
recursive group against the generated prelude, ignoring file offsets. Invoke
this before validation/linking/instantiation, since a start function can perform
effects. A manifest/prelude check does not prove function semantics; engine
validation and import linking remain required. Existing CLI/AOT artifacts do not
yet use this gate; dependency graph/target metadata belongs to integration.

The original Rust runtime and encoder tests retain the repository license; no
upstream ClojureScript implementation was copied. Java/Node are not runtime
dependencies, and this module has no host imports or canonical linear memory.

Validation command:

```sh
cargo test -p suss-compile --test runtime_abi --locked -- --test-threads=2
```

Seven executing tests independently inspect heap fields/units and verify signed
zero, infinity, NaN payload storage, binary64 rounding, lone surrogates, astral
pairs, checked writes, forced GC, shared types across runtime/producer/consumer,
old captures after binding replacement, fixed/variadic arity, and rejection before
initializer effects. A valid i64 Number-layout mutation with an unchanged
manifest must fail before initialization. Full source compatibility, reader
metadata/spans, lowering dominance/effects, nominal protocols, complete exceptions,
scheduler, target adapters and persistent REPL are not established by these tests.

The portable resolver now emits exact shared cell imports. `binding-unbound`,
checked `binding-get` and binding-set bound-state transitions distinguish an
uninitialized var from nil; see [portable resolution](portable-resolution.md).
Ten executing resolution tests supplement the seven ABI tests. Production
namespace loading, source definitions and universal call lowering remain open.

Fixed source closures and generic local/global/computed calls now lower through
the shared universal ABI; central invocation checks non-callable and malformed
argument arrays before casts. Eleven source/IR tests supplement the prior suites;
see [closure lowering](portable-closures.md). Extended signatures, source namespace
loading and production migration remain incomplete.

Source definitions now use explicit bound checks/writes through this ABI. Ten
executing definition tests verify nil/false defonce state, skipped effects and
failed initializer publication; see [definition lowering](portable-definitions.md).
The ten-type layout is unchanged; binding-bound is an additional private intrinsic.
Production session/module loading and migration remain incomplete.
