# Persistent native session host

The native `suss_cli::portable_session::Session` embedding API owns one Wasmtime
Store, shared production runtime, compiler Environment, binding cells, initialized
module identities and resident fragment instances. It uses the portable pipeline;
it does not rebuild state by replaying source. The legacy command/REPL frontend
has not migrated, and this increment does not complete M3 acceptance.

## Execution and recovery

`Session::eval` prepares one input and its leading `ns` dependencies together.
Compiler `modules::prepare_input` discovers dependencies with the same namespace
header grammar as ordinary modules, compiles them from immutable snapshots, restores
the caller's namespace scope and compiles the input against staged declarations.
A source error anywhere discards the entire plan before host bindings or effects.
The optional input namespace may change the active scope when execution is staged;
ordinary later inputs preserve its aliases/refers.

The host validates every artifact and compiles every Wasm module before allocating
new cells. It stages linker imports, reuses existing cells by Global identity and
instantiates all fragments before publishing compiler state or invoking eval.
Generated portable fragments have no start initializers: source effects run only
through explicit eval calls. Modules execute in dependency/require order and are
marked provided only after successful initialization. `load_namespace` preserves
the caller's current scope and returns None for an already initialized module.
An ordinary namespace catalog entry does not make a file loaded.

A failed definition initializer leaves its previous binding; earlier effects and
successful dependencies remain. Failed module identities are not marked loaded,
so retry compiles that module and reuses successful dependencies. Linked instances
are retained even when their initialization fails or a later initializer never
runs; this conservative residency policy lasts until reset.

`SessionError` distinguishes source diagnostics, dependency diagnostics, exact
shared-tag language exceptions, engine traps and host failures. Language payloads
remain owned values; pending exception state is taken before returning to the
native caller. Foreign tags are host failures, never fabricated language success.
Inspection callbacks follow the same exception-state normalization. Fuel exhaustion
is an engine trap, distinct from wrong arity/type language errors. A fresh operation
budget lets the next input execute after either kind of error. This is synchronous
runtime recovery, not cancellation or an asynchronous scheduler.

## Owned values and lifecycle

Every returned `SessionValue` owns its GC root. Values and captures survive later
inputs, rebinding and explicit `collect`. Clone retains another owned handle;
dropping a handle releases it. `invoke` uses the shared runtime argument array and
central arity checks. Values must belong to the same session generation; another
session or a reset produces ForeignValue before native GC access or guest calls.

```rust
use suss_cli::portable_session::{Session, SessionError};
fn main() -> Result<(), SessionError> {
let mut session = Session::new()?;
session.eval("(def f (let [x 1] (fn [y] x)))")?;
let old = session.eval("f")?;
session.eval("(def f (fn [y] 10))")?;
let argument = session.eval("7")?;
let _result = session.invoke(&old, &[&argument])?; // original captured 1
Ok(())
}
```

`inspect` supports native observations/interop in a bounded root scope. Return
ordinary host data or owned observations from its callback; raw Wasmtime Rooted
references become invalid when that scope ends. It is not a language printer or
an opaque-object success decoder. Tests inspect exact numeric fields, UTF-16 units,
sentinel values and exception descriptors independently of Suss equality/printing.

`reset` constructs a replacement before dropping the old Store. It preserves
options/engine, discards bindings, namespace scopes, loaded identities and resident
instances, and invalidates old handles. Old external handles may outlive reset,
but cannot access the new Store. Core provisioning is the explicit bounded
bootstrap intrinsics, not the complete portable core library.

`SessionStats` exposes resident instance count, the sum of their input artifact
sizes, binding cell count, successfully loaded module count, current-generation
external value handle count and allocated GC heap capacity. Artifact sizes are not
actual JIT bytes or retained raw Wasm. Handle counts exclude internal cell/runtime
roots; heap capacity is not live-object usage. These counters separate residency
from caller-owned roots but do not prove absence of leaks or full M3-04 acceptance.

Options supply source paths and one fuel budget per complete input/load/invocation,
including its dependencies. A shared configured engine is reused. Caller-supplied
engines must enable GC/function references/tail calls/exceptions and fuel.

## Evidence and limits

Eleven integration tests plus two private host regressions execute real fragments,
rooted values and shared cells: once-only initialization/defonce, bound nil/false,
old captures/live rebinding, UTF-16 after GC, compile failure isolation, exact-tag
initializer recovery, foreign/reset handles, dependency transaction compilation,
reverse-order diamond effects, successful dependency reuse after failure, canonical
core provisioning, scope/catalog distinctions, artifact/import gates, foreign
callback exception recovery and fuel trap recovery. Existing 65 focused compiler
checks also pass. No new test is ignored.

Initial API regression compilation failed because the native library host did not
exist. Two initial fixtures then failed with the existing located unsupported
numeric diagnostic; globals/parameters do not yet have dynamic arithmetic lowering.
The persistence fixtures use literal writes and captures to isolate replay/ownership
behavior. Those failures were not skipped and do not establish a numeric repair.

Implementation/tests are original; pinned EPL-1.0 ClojureScript namespace/definition
semantics inform the portable contract, with no copied upstream implementation.
Only the already-locked tempfile package is added as a CLI development dependency;
no package version changes or shipped JVM/Node dependency are introduced. The
existing 42-case portable oracle and legacy 9-pass/7-failure/0-skip baseline remain
separate evidence, and all 1,065 inventory items remain unassessed.

Production REPL/command frontend migration and printing, atoms/types/collections,
extended closure signatures and dynamic arithmetic/core coercions, complete
ExceptionInfo/effect/recur IR, reload/cache/privacy policy, compiled macro sessions,
async I/O/cancellation and live heap accounting remain unfinished. Unsupported
source forms still return located diagnostics. Source declaration/unbound-var
semantics are not fully certified. Do not close #10/#12/#13/#15 from this API alone.
Next connect the actual REPL frontend to this host as portable lowering/printing
coverage reaches replacement acceptance; retire source replay after that gate.
