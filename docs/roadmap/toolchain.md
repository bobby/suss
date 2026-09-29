# Toolchain evidence, 2026-09-29

The current M0-02 worktree pins Wasmtime/wasmtime-wasi 49.0.1,
wasm-encoder/parser and wit-parser/component 0.258.0, wit-bindgen 0.61.1,
and the transitive WAT parser 1.258.0 in Cargo.lock. Native compilation and
focused executing fixtures and the full migrated workspace baseline pass.
Do not describe this workspace as a WASI 0.3.1 compiler yet.

The candidate CLI tools were downloaded from official release assets into /tmp:
[Wasmtime 49.0.1](https://github.com/bytecodealliance/wasmtime/releases/tag/v49.0.1)
and [wasm-tools 1.258.0](https://github.com/bytecodealliance/wasm-tools/releases/tag/v1.258.0).
Installed tools were not replaced. The current worktree now migrates the Rust
dependency family separately, with focused execution evidence below.

[Machine-readable observations](toolchain-probes.json) distinguish execution from
parsing/compilation. GC, typed function references, tail calls and exceptions each
execute and return 42. Map, future/stream and implements component type declarations
compile with explicitly selected features. The WIT fixture parses and prints maps,
async functions, future/stream and external-id. These are narrow probes; they do
not demonstrate canonical async transfers or actual map values crossing a boundary.

```sh
python3 scripts/probe_toolchain.py \
  --wasmtime /path/to/wasmtime-49.0.1 \
  --wasm-tools /path/to/wasm-tools-1.258.0 \
  --output docs/roadmap/toolchain-probes.json
cargo test -p suss-compile --test shared_runtime
```

Three shared-runtime probes now pass with Wasmtime 49.0.1. Matching recursive
type groups share closures and rooted objects across independently compiled
fragments in one Store. Forced GC preserves old objects and captured closures;
shared binding lookup observes a replacement while the captured closure keeps
its old behavior. Descriptor identity distinguishes two same-layout nominal types.

A probe-only loader checks the declared ABI before instantiation. A mismatch
cannot run the module initializer. The engine also rejects a different recursive
layout even when the version label matches. Deliberately bypassing the ABI gate
makes the rejection test fail; restoring it passes. The local M0-03 feasibility
gate is now evidenced on the selected engine. This is not the production runtime
ABI, manifest format or incremental REPL; those remain M2/M3 acceptance work.

The official WIT graph is now vendored byte for byte under `vendor/wasi/`.
[The lock](wasi-wit-lock.json) pins the release commit/archive and all 15 WIT
file hashes, with original versions and local dependency copies preserved.
[Package probes](wasi-wit-probes.json) record successful resolution and validated
binary WIT round-trips for all six official packages with wasm-tools 1.258.0.
This is package encoding evidence, not execution of WASI capabilities.

```sh
python3 scripts/wasi_lock.py
python3 scripts/wasi_lock.py --wasm-tools /path/to/wasm-tools-1.258.0 \
  --output docs/roadmap/wasi-wit-probes.json
```

The Rust `toolchain_profile` suite now executes core GC, typed function
references, tail calls and exceptions, transfers actual map values through
canonical guest memory in both directions, and calls a named implements import
from a guest. An external-id annotation survives WIT metadata encoding and
selects an export at runtime; the selected export executes and returns 42. All
eight tests pass. The 28 component tests and shared-runtime
fixture also pass on the new engine. These are feasibility/legacy fixtures, not
Suss support for the full boundary graph. Unimplemented Suss boundary types and
async functions now produce explicit Unsupported diagnostics.

Eight `toolchain_async` tests execute stackless canonical callbacks and typed
future/stream endpoint transfers. Canonical imports actually return Pending,
complete via a waitable callback, and can be cancelled from the guest. Separate
future and stream probes now read payloads from guest memory: the future returns
u32::MAX, and one-byte stream reads preserve `[0, 255, 42]` and explicitly observe
EOF. The stream producer sees capacity one, produces only on demand and retains
no read-ahead buffer. These are narrow demand/backpressure checks, not arbitrary
buffer or stress tests.

Future and stream read cancellation return the exact CANCELLED event with zero
items, ask the Rust producer to finish, and leave the guest memory sentinel
unchanged. Async cancellation requires the separate
`wasm_component_model_more_async_builtins(true)` feature; enabling only component
async rejects the subtask cancellation fixture during validation. The adapted
callback fixture retains upstream license and hash provenance under
`crates/suss-compile/tests/fixtures/`; the new read/cancel fixtures are original.

```sh
cargo test -p suss-compile --test toolchain_profile --test toolchain_async \
  --test shared_runtime --locked -j2 -- --test-threads=2
```

The local M0-02 named feasibility acceptance probes now pass. Issue #2 remains
open while publishing the local commits/results requires approval. See the
criterion-by-criterion audit in the handoff. This does not establish generated
Suss boundary adapters, nested async values, arbitrary payload shapes, endpoint
write cancellation or production Suss scheduling. These belong to the M5/M6
acceptance suites.

Wasmtime 49.0.1 has no public API to cancel an individual started host call;
its referenced upstream issue #11833 remains open. Dropping a Rust call future
is not cancellation. The executing guest-driven cancellation paths above do
not establish top-level host interruption or Suss session cancellation.
The optional Wasm compiler-component build fails on native-only CLI imports;
see the handoff for its failed command and limits. The production bundled WIT remains
in use pending replacement binding tests.
M0-04 now has an actual browser fixture in `tests/browser/`. Chrome
154.0.8037.58 loaded the ES module and core-GC module, retained a continuation
across Promise suspension, and passed cancellation/stale-callback checks.
[The result](browser-probe.json) records source hashes. Chrome emitted the
completed passing DOM but hung during display teardown; the harness terminated
the isolated process after 20 seconds. This limitation remains explicit.

```sh
python3 scripts/probe_browser.py --chrome /path/to/chrome \
  --wasm-tools /path/to/wasm-tools-1.258.0 \
  --output docs/roadmap/browser-probe.json
```

The server binds only to localhost and Chrome uses a temporary isolated profile.
This checks a hand-written continuation fixture, not compiled Suss, browser-wide
compatibility, or the canonical ABI. Jco packaging comparison remains M0/M8 work.
