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

`Session::interrupt_handle()` (and `CompiledMacros::interrupt_handle()` for the
macro phase) returns a thread-safe `InterruptHandle`. `interrupt()` traps the
session's running operation with `Trap::Interrupt`: `SessionError::is_interrupt()`
is true and the error displays as `Interrupted`. The shared engine enables epoch
interruption; each Store's epoch callback traps only when its own session's
request is set and consumes it, so other sessions on the engine keep running and
post-trap dynamic restoration completes. A request made while idle is discarded
when the next operation starts, and handles stay valid across `reset`. As with
fuel traps, effects before the interrupt are not rolled back and the next input
executes normally ([`session_interrupt.rs`](../../crates/suss-compile/tests/session_interrupt.rs)).
This interrupts running synchronous code only; cooperative cancellation of
pending I/O needs the design section 9 scheduler.

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

`reset` constructs a replacement before requesting cancellation in the old Store.
It requests cooperative cancellation of all tasks and registered native host
operations, then drives at most 64 fuel-budgeted scheduler turns. Replacement
requires zero pending task owners, registrations, queued work and host requests.
If cleanup awaits an unknown producer or exceeds that bound, it returns
`SessionError::ResetPending`: the old Store, bindings and handles remain usable
for completing cleanup and retrying reset. Cancellation effects already performed
are retained; reset retries do not replay source initializers or host hooks.
The compiled two-phase REPL uses the same retirement gate for both Stores before
replacing either. A failed replacement preparation starts no cancellation.

`pending_future_with_cancel` registers an internal `OwnedRooted` future and a
`FnOnce() + Send + 'static` cancellation hook. The hook must be nonblocking,
must not execute Suss source, and should request host-operation abortion without
waiting for it. Reset consumes it before invocation, at most once across retries.
Terminal completion releases the registry entry without invoking its hook.
`pending_future` uses the same registry with a no-op host hook. These native
storage APIs do not prove canonical callback transport or disposal of arbitrary
host resources. `SessionStats::pending_host_requests` counts registry roots
separately from caller-owned `external_value_handles`.

Fresh-bootstrap native reset validation on 2026-10-07 passes all three focused
unit regressions: unknown-producer preservation/retry and cancellation-catching
cleanup pass 2/2 in 6.55s; independent registry ownership/normal completion passes
1/1 in 0.19s, with zero failures or ignores. The former verifies both direct and
compiled REPL retirement gates, explicit completion in the retained old Store,
exactly-once host-hook invocation and captured read-guard release, actual
cancellation caught in `finally`, and rejection of late handles after replacement.
The latter distinguishes internal roots from external handles and checks that
normal completion does not invoke cancellation. Root's separate native
`session_lifecycle_reset_pending` integration also passes 1/1 in 0.19s.
The dedicated [`session_lifecycle_reset_interruption.rs`](../../crates/suss-compile/tests/session_lifecycle_reset_interruption.rs)
integration passes 1/1 in 0.16s, with zero failures, ignores or filtered tests.
Its consumed host hook invokes the real interrupt handle immediately before
runtime cancellation: reset returns `Trap::Interrupt`, the future remains Pending
and the old closure remains callable. After budget refresh, reset retries without
replaying the hook or releasing its captured guard twice, and rejects late old
handles after replacement. No sleep or synthetic interruption result is used.
Commands and logs are recorded in the [handoff](../roadmap/handoff.md).

These results cover native reset storage and cleanup only. Task cancellation
still lacks an operation-to-waiter ownership association for pending host I/O;
it must not abort a dependency shared by another task. Canonical per-invocation
cancellation, callback/resource disposal and final integrated acceptance remain
unproven.

A successful reset preserves
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


## Native pending-I/O profile

The Unix command REPL installs an explicit Runtime-only native profile before
source preparation. `suss.io/read-byte(path)` immediately returns a nominal GC
future and queues owned argument/completion roots. The Store owner opens a fresh
endpoint with `O_NONBLOCK`, reads one byte, and closes it before publishing the
outcome. Ready contains a number in `0..255`, or nil for EOF. Each call opens a
new endpoint; this operation is not a shared stream cursor. Native open/read
failures produce genuine Failed ExceptionInfo values, distinct from canonical
WIT `result::err`. Source-owned namespaces and existing source bindings cannot
be silently replaced by profile installation.

The data endpoint must differ from command stdin, checked by device/inode after
opening; aliases and symlinks to the command endpoint are rejected before read.
The adapter never reads command input, runs source from a host callback, or
settles completions on a background thread. `O_NONBLOCK` does not guarantee
bounded regular-file or filesystem latency. Other platforms and general streams,
backpressure, multi-byte transfers and WASI interoperability retain their design
acceptance requirements.

The frontend polls input and native completions between bounded scheduler turns.
Ctrl-C requests cooperative task cancellation; awaited finally work may suspend
and receives I/O service before retirement. Reset and exit drain source owners
and native endpoints before discarding old roots. Reset reinstalls the profile
with fresh Store identities; late old handles cannot publish into the replacement.
Printing a future observes nominal status without implicit await, source execution
or terminal-payload rooting. Resident code counters remain distinct from the
test-only actual post-GC live-byte probe.

Current evidence and limitations are recorded in the milestone issues and handoff.
The staged native implementation does not certify all M3 acceptance criteria.


## Portable bounded stream contract (implementation under validation)

The original `suss.async` GC stream profile is separate from canonical WIT stream
transport. `stream-pair` takes an integer capacity from 1 through 4096 and returns
`[reader writer]`. These are unique endpoint capabilities; aliases refer to the
same endpoint, and there is no operation that creates an independent owner.
Each endpoint permits one pending operation. A second read or write fails without
replacing the earlier operation.

`read-chunk reader limit` and `write-chunk writer vector` return task-owned
futures. Limits and nonempty vector counts must be at most the capacity and 256.
A write copies its immutable input inside the rooted task, yielding at loop
backedges before claiming its endpoint. Cancellation during copying leaves the
endpoint unclaimed. It accepts its entire chunk when enough capacity is available; otherwise it
waits and provides bounded backpressure. Reads return nonempty immutable vectors
or a private EOF value recognized by `stream-eof?`. A nil element remains data:
`[nil]` is distinct from EOF. Completion never invokes source inline.

Closing the writer rejects unaccepted writes, drains already accepted data, then
returns EOF on reads. Closing the reader discards buffered data and fails pending
operations. `fail!` accepts only the writer, discards data and preserves its exact
failure payload. Writer close/failure follows the first terminal choice; repeated
terminal requests return false. Cancelling an operation withdraws its pending
request without closing its endpoint. Already committed acceptance or consumption
is an effect and is not rolled back by later cancellation.

Private operation journals remain rooted through prepared transfer, snapshot
publication, future settlement and retirement. Scheduler recovery must retire an
orphan operation when a Wasmtime trap prevents source `finally` from executing.
Reset checks both scheduler tasks and stream operation roots before replacing the
Store; old endpoint values then fail the ordinary foreign-value check. These
requirements need the executing stream, fuel-interruption and GC regressions;
staged code and bootstrap generation alone do not establish acceptance.

Native frontend interruption during a pure task/status observation waits for the
input signal acknowledgment before retrying. Unknown task counts keep cleanup
and buffered input gated. A completed input result remains rooted and source
effects are never replayed while tracking retries; reset retains its pending
intent across an interrupted second preflight. Non-interrupt failures after a
completed submission terminate conservatively rather than leave a stuck prompt.
This observation path does not classify uncertain scheduler mutation or teardown
as recovered.
