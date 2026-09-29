# Toolchain evidence, 2026-09-29

The production workspace is still the prototype toolchain: Wasmtime 39.0.1,
wasm-encoder/parser and wit-parser/component 0.221.3, locked by Cargo.lock.
Do not describe this workspace as a WASI 0.3.1 compiler yet.

The candidate CLI tools were downloaded from official release assets into /tmp:
[Wasmtime 49.0.1](https://github.com/bytecodealliance/wasmtime/releases/tag/v49.0.1)
and [wasm-tools 1.258.0](https://github.com/bytecodealliance/wasm-tools/releases/tag/v1.258.0).
No installed tools or production dependencies were replaced.

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

The shared-runtime test uses the current Rust dependency. It proves that modules
with matching recursive type groups share closures and rooted objects across GC,
and that descriptor identity distinguishes two same-layout types. Repeat this
probe with the candidate engine before migrating the REPL. This is a feasibility
fixture, not the production runtime ABI.

M0-02 still requires the official WIT package graph/content locks, candidate Rust
dependency migration, canonical async execution and bidirectional map transfers.
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
