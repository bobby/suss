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
open during draft PR #40 acceptance review. See the
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
M0-04 executes the full required core feature probe set in Chrome
154.0.8037.58: GC, typed function references, tail calls and exceptions return 42.
The ES module bridge retains a GC continuation through Promise suspension,
cleans up cancellation and isolates stale callbacks. A malformed artifact checks
that a compile rejection preserves both the required feature name and original
engine diagnostic; it is not mislabeled as a valid unsupported feature.

The optional Jco 1.35.0 route transpiles a small GC component and loads its
ES module/core Wasm in Chrome; its typed u32 export returns 42. This certifies
simple GC component packaging, not async WIT values, WASI capabilities, nested
boundaries or the production Suss browser artifact. Direct core loading remains
the minimum browser route; Jco is separately tested optional packaging. Its
[instantiation API](https://github.com/bytecodealliance/jco/blob/main/docs/src/transpiling.md)
lets the browser loader provide modules/imports explicitly.

[The result](browser-probe.json) records source/harness hashes, generated Jco
artifact hashes and each required check. Python rejects incomplete or unknown
results; a Jco result is required only when that route was requested. Chrome
again emitted complete passing DOM but timed out during teardown. The probe
records a semantic pass and a null process exit code, with termination explicitly
recorded; clean process shutdown is not claimed.

```sh
python3 scripts/probe_browser.py --chrome /path/to/chrome \
  --wasm-tools /path/to/wasm-tools-1.258.0 --jco /path/to/jco-1.35.0 \
  --output docs/roadmap/browser-probe.json
```

Jco was installed with install scripts disabled under /tmp for this development
probe; it is not a shipped dependency. The HTTP server binds only to localhost
and Chrome uses a temporary isolated profile. Other browsers, production typed
host bindings, canonical async browser interoperability and distributable browser
artifacts remain M8 acceptance work.
