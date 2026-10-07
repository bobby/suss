# Persistent native session host

The native `suss_cli::portable_session::Session` embedding API owns one Wasmtime
Store, shared production runtime, compiler Environment, binding cells, initialized
module identities and resident fragment instances. It uses the portable pipeline;
it does not rebuild state by replaying source. The native command REPL now uses
this host for bounded compiled input and scalar display; see
[frontend evidence and limits](compiled-repl.md). The REPL core profile now supports [persistent atom storage](atoms.md). The
native eval/file and namespace/project/main AOT command routes now use compiled
source preparation, with executing component evidence. The public legacy compiler
and component evaluator still need retirement under #14. Issue-specific persistence
and namespace evidence is mapped in [the acceptance audit](m3-session-acceptance.md).
Complete core/macros and M3 lifecycle acceptance remain unfinished. Older progress
notes below retain the API's development history; their old counts and pending
frontend statements do not supersede the current acceptance audit.

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
Inspection callbacks follow the same exception-state normalization. Translated host
errors clear pending exceptions, and a callback that returns success while leaving
a pending exception is rejected as a host error. Fuel exhaustion
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
but cannot access the new Store. Minimal sessions provision bounded bootstrap intrinsics. `Session::new_repl()`
also compiles the provenance-tracked core artifact once and reprovisions it on
reset. The artifact and its dependencies do not establish complete portable core
compatibility.

`SessionStats` exposes resident instance count, the sum of their input artifact
sizes, binding cell count, successfully loaded module count, current-generation
external value handle count, allocated GC heap capacity and numeric memory capacity. Artifact sizes are not
actual JIT bytes or retained raw Wasm. Handle counts exclude internal cell/runtime
roots; heap capacity is not live-object usage. These counters separate residency
from caller-owned roots but do not prove absence of leaks or full M3-04 acceptance.

Live GC heap bytes (the Wasm GC heap only, not host tables, session maps or
numeric scratch memory) are measured separately, in tests only. Pinned Wasmtime 49.0.1
has no public live-heap counter, but after each completed collection
`runtime/vm/gc.rs` logs the exact `GcHeap::allocated_bytes()` result at trace
level. [`session_live_heap.rs`](../../crates/suss-compile/tests/session_live_heap.rs)
installs a test-only logger that reads exactly that record for one synchronous
`Session::collect` on the calling thread; a missing, duplicate or malformed record
fails. In both the Runtime and Macro Stores it shows that:

* live bytes return exactly to the baseline after a handle-held graph, a
  self-referential atom and a graph retained only through a global cell are released;
* live bytes stay above the baseline while those graphs are retained, by an
  identical amount in every round;
* resident fragments, artifact bytes, cells, modules and handles stay unchanged;
* after repeated `reset`s following retained state, the replacement Store's live
  bytes and resident code equal a fresh session's, so nothing accumulates across
  generations (the test does not observe the old Store's drop itself).

The collector `Collector::Auto` selects for this build (copying) reclaims cycles; the
cycle assertion would fail under a collector that does not. `SessionStats` deliberately has no
live-bytes field: no public Wasmtime API supplies one, and a production logger
would be process-global.

Options supply source paths and one fuel budget per complete input/load/invocation,
including its dependencies. A shared configured engine is reused. Caller-supplied
engines must enable GC/function references/tail calls/exceptions and fuel.

## Evidence and limits

Seventeen integration tests plus four private host regressions execute real fragments,
rooted values and shared cells: once-only initialization/defonce, bound nil/false,
old captures/live rebinding, UTF-16 after GC, compile failure isolation, exact-tag
initializer recovery, foreign/reset handles, dependency transaction compilation,
reverse-order diamond effects, successful dependency reuse after failure, canonical
core provisioning, scope/catalog distinctions, artifact/import gates, foreign
callback exception recovery (including translated errors and swallowed throws)
and fuel trap recovery. Existing 73 focused compiler
checks also pass. No new test is ignored.

Initial API regression compilation failed because the native library host did not
exist. Two initial fixtures then failed with the existing located unsupported
numeric diagnostic; this preceded primitive dynamic arithmetic lowering.
The original persistence fixtures use literal writes and captures to isolate
replay/ownership behavior. Subsequent regressions now execute arithmetic over live
cells and parameters, preserve once-only operand effects and independently decode
primitive conversions. Unsupported object coercion preserves completed effects
and the previous failed-initializer binding.

Implementation/tests are original; pinned EPL-1.0 ClojureScript namespace/definition
semantics inform the portable contract, with no copied upstream implementation.
Only the already-locked tempfile package is added as a CLI development dependency;
no package version changes or shipped JVM/Node dependency are introduced. The
existing 245-case portable oracle and legacy 9-pass/7-failure/0-skip baseline remain
separate evidence, and nine arithmetic/nominal declarations are reviewed as in-progress and 1,056 remain unassessed.

Production REPL/command frontend migration and printing, atoms/types/collections,
extended closure signatures, object coercions and complete source core/macros, complete
ExceptionInfo/effect IR, reload/cache/privacy policy, compiled macro sessions,
async I/O/cancellation remains unfinished; live heap accounting is test-only (above). Unsupported
source forms still return located diagnostics. Source declaration/unbound-var
semantics are not fully certified. Do not close #10/#12/#13/#15 from this API alone.
Next connect the actual REPL frontend to this host as portable lowering/printing
coverage reaches replacement acceptance; retire source replay after that gate.

Numeric memory capacity includes Rust stack/static data and conversion scratch,
not live GC objects or JIT code. It grows to a checked high water mark and stays
bounded for repeated same-sized conversions until reset replaces the Store.
Tests inspect unchanged original UTF-16 strings, exact allocation-failure language
exceptions and successful next inputs after fuel traps. A private regression
interrupts an actual Rust helper frame and verifies the following conversion resets
its stack pointer; a missing-reset mutation fails that regression. Trusted inspect
callbacks use the Store's remaining fuel rather than starting a new operation budget.

Arithmetic values now read canonical live cells, including higher-order/computed
calls and old captures after rebinding/GC; see [arithmetic values](arithmetic-values.md).
Four bootstrap arithmetic cells are included in binding_cells from startup and
reprovisioned on reset. Caller evaluation of callee/arguments remains once in order.

Source loop/fixed-function recur executes through this host with parallel replacement,
old iteration captures, changing types and fuel recovery; see [recurrence](recurrence.md).
The native focused scope is 27 tests (23 integration/four private).

Named/multiple fixed signatures now retain exact self identity, captures and old
behavior after GC/rebinding, with typed arity-hole errors after ordered operands;
see [closure signatures](closure-signatures.md).


Ten nominal source regressions bring the current native focus to37 (33 integration,
four private). Descriptor-backed objects/classes/protocol keys remain rooted across
fragments and GC; old class values retain identity, old generated arrows read current
type cells, and old captured protocol dispatchers observe later table updates.
Compile failures publish no staged class/arrow/key bindings. Runtime nominal errors
leave the prompt usable. [Nominal source support](nominal.md) remains bounded; the
command frontend and full M3/M2-04 acceptance are still incomplete.

Ten additional [source exception regressions](exceptions.md) execute exact payloads, handler captures/GC, ordered cleanup, divergent operands and failed publication. Together with private4 and existing integration33, the current native session gates total47 passing tests. ExceptionInfo/core error surfaces, dynamic binding and asynchronous interruption remain unfinished.

## Core initialization artifact preparation

The native host now instantiates the portable compiler's core binding
artifact instead of manually creating the initial cells through Rust calls.
The emitted module supplies the same phase-qualified cells, initializes the
ExceptionInfo class before captured functions, and provisions self-cell closures.
Focused execution passes; independent review/full baseline/final CI remain
required. See [the artifact boundary and remaining AOT
work](compiled-core-bindings.md). Base runtime/initializer input byte sizes are
reported separately from installed user fragment artifact bytes. Neither counter
is a JIT-memory or live-GC measurement.
