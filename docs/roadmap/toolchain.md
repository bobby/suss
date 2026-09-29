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

The shared-runtime test now passes with Wasmtime 49.0.1. It proves that modules
with matching recursive type groups share closures and rooted objects across GC,
and that descriptor identity distinguishes two same-layout types. This probe is now repeated on the candidate engine; full production REPL
acceptance is still future work. This is a feasibility
fixture, not the production runtime ABI.

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
from a guest. All seven tests pass. The 28 component tests and shared-runtime
fixture also pass on the new engine. These are feasibility/legacy fixtures, not
Suss support for the full boundary graph. Unimplemented Suss boundary types and
async functions now produce explicit Unsupported diagnostics.

M0-02 still requires canonical callback async/cancellation and future/stream
execution, plus executing external-id semantics.
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
