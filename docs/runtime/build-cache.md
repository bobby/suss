# Persistent build directory and compilation caches

Suss persists compilation work across processes in a **build directory** (#272).
Every cache entry is disposable: deleting the directory, or any part of it, only
makes the next build slower. The implementation is
`crates/suss-compile/src/build_cache.rs` and its `inputs` and `builds` modules.

## Selecting the build directory

Precedence, highest first:

1. `--build-dir <DIR>` or `--no-cache`. These are global options and come before
   the command, as in `suss --build-dir out/suss compile ...`.
2. `SUSS_NO_CACHE=1` disables caching. Otherwise `SUSS_BUILD_DIR=<DIR>` selects a
   directory.
3. In project mode, `:build-dir "<DIR>"` in `deps.sus`. It resolves relative to
   the directory containing `deps.sus`, as `:src-paths` do.
4. `target/suss` beside the nearest `deps.sus` at or above the project
   directory, or the working directory outside project mode.
5. The user cache directory: `$XDG_CACHE_HOME/suss`, else `~/.cache/suss`. On
   macOS it is `~/Library/Caches/suss`; on Windows, `%LOCALAPPDATA%\suss`.

Relative directories resolve once against the starting working directory.

Library embedders (`Session`, `CompiledMacros`, `portable_aot`) get no
persistent cache unless they call `build_cache::configure` or set
`SUSS_BUILD_DIR`. This workspace's `.cargo/config.toml` sets `SUSS_BUILD_DIR` to
`target/suss`, so every test binary and `cargo run` shares one warm cache.

`suss cache info` prints the selected directory and its cached size.
`suss cache clean` removes cached state.

## Layout and separation from outputs

```text
<build-dir>/
  CACHEDIR.TAG                        marks a Suss build directory
  cache/v1/<layer>/<k0k1>/<key>       validated entries
  tmp/                                in-progress writes
```

Final outputs (`-o`, a world's `:output`) belong outside the build directory.
Suss does not prevent pointing one there, but `suss cache clean` would leave it
alone only if it is outside `cache/` and `tmp/`. The cache keeps its own
content-addressed copy of each reusable output.

A build never records observations inside the active build directory. Source
roots or a WIT directory that contain it, such as `-w .` with the default
`target/suss`, therefore do not invalidate their own builds.

`suss cache clean` refuses a directory without the Suss `CACHEDIR.TAG`. In a
tagged directory it removes only `cache/` and `tmp/`, so it can never delete
source or unrelated files, even when pointed at the wrong directory.

## Cache layers

### Native code (`native-module`, `native-component`)

This is Cranelift output for every core module a session instantiates: the
runtime ABI module, core bindings, the shipped bootstrap and each compiled
fragment. It also covers the component that `suss run` executes.

- **Key:** Wasmtime's engine compatibility hash plus the SHA-256 of the exact
  Wasm bytes. The compatibility hash covers the Wasmtime version, target and
  code-affecting configuration.
- **Cost of a miss:** in measured sessions this layer is more than 99% of
  startup time.

### Emitted fragment Wasm (`fragment`)

This is the persistent tier of the in-memory `ArtifactCache`.

- **Key:** the same as the in-memory cache, which covers:
  - the compiler source digest, runtime ABI and target/profile;
  - phase, source text and path, forms and IR;
  - the macro dependency records.
- **Validation:** a disk hit is checked as an in-memory hit is: the entry
  digest, `runtime_abi::verify_artifact` and `artifact_identity::verify`. Any
  failure is a miss that recompiles.

### Whole builds (`build-manifest`, `build-output`)

Used for `suss compile` in every mode.

- **Manifest key:** the compiler source digest, a digest of the embedded core
  source, import manifest, versioned bootstrap artifacts and lazy-seq source,
  the runtime ABI, target/profile, the working directory and every semantic
  argument. The embedded digest matters because core can be regenerated without
  any Rust change. These are the mode,
  source/namespace, WIT path and world, source roots, export mappings,
  configuration path, selected world and `--compile-fuel`.
- **Manifest format:** canonical EDN (`docs/runtime/tooling-edn.md`) with
  `:format-version 2`. The version is also part of the key, so JSON manifests
  from version 1 are misses, as is any entry that does not parse.
- **Manifest contents:** every filesystem observation the build made, and the
  SHA-256 of each output:
  - the contents of each file read: inputs, discovered modules, macro
    namespaces, `deps.sus` (from the read the build used), and every file in
    the WIT package tree. Each real directory is visited once, so symlink
    cycles terminate;
  - each namespace candidate path probed, with its kind and canonical target,
    and each symlinked ancestor of a missing candidate. Adding a shadowing file
    or swapping or removing a symlink therefore invalidates the build;
  - each directory listed.
- **Hit:** a later request with the same key reuses the outputs only if every
  observation still holds. Outputs are stored by content digest.
- **Failures:** only successful builds are recorded. A failed build always
  recompiles, so diagnostics are never replayed.

A changed macro namespace invalidates every build that loaded it, even when the
using source is unchanged, because its file contents are a recorded input.

### Intermediate stages that are not persisted

Parsed forms, analysis environments, HIR and IR are not persisted. They are not
serializable, and their construction runs macros with effects. Each partial
build therefore rereads and reanalyzes its source and reruns macro expansion.
This preserves:

- compile-time effects and evaluation order;
- `defonce` and initializer semantics;
- the cross-unit gensym, `#()` parameter and reify-class counters, and with
  them byte-identical output.

Only emission and native compilation are skipped for units already completed.

## Checkpoints and interrupted builds

Each entry is published by writing a uniquely named temporary file under
`tmp/` and renaming it into place. Readers therefore see the previous complete
entry or the new one, never a partial file. Entries are not fsynced: after a
crash, a torn entry fails its digest and is a miss.

The checkpoint granularity is:

- one native module or component;
- one emitted fragment: a top-level form, a source module, or a macro definition;
- one whole build. Its manifest is published last and refers only to outputs
  already stored.

An interrupted or failed build keeps every unit it completed. The next build
reuses those units' native code and emitted Wasm, and recomputes reading,
analysis and expansion as described above. Arbitrary instruction-level
resumption is not provided.

A cancelled writer leaves at most an orphaned temporary file. Pruning removes
temporaries older than an hour.

Final outputs are written the same way, through a sibling temporary renamed over
the output. A failed or interrupted compilation or `wasm-opt` therefore leaves
the previous output intact, and never exposes a partial component.

- A symlinked output is written through to its target.
- An existing output keeps its permissions.
- A non-regular target, such as `/dev/stdout`, is written directly.
- A writer killed mid-write can leave a `.tmp-*` file beside the output.

## Validation, corruption and concurrency

Every entry starts with a header that is checked before use: a magic number,
the layer name, the full key, the payload length and the payload's SHA-256.

- **Recoverable misses:** a truncated, extended, corrupted, wrong-layer or
  wrong-key entry, an unreadable file, a stale manifest, and a payload that
  Wasmtime rejects as incompatible. The caller recomputes and republishes.
- **Diagnostics:** set `SUSS_CACHE_TRACE=1` to print every hit, miss, store,
  stale manifest and rejection, with its reason, to stderr.
- **Concurrent writers:** they publish identical content for the same key, and
  the last rename wins. A pruner that races a reader causes only a miss.

## Trust policy for native code

Loading serialized native code executes it, so integrity digests are not
enough: anyone able to write an entry could forge its digest. On Unix, a native
entry is loaded only if the entry and every directory from it up to the build
root:

- belong to the current user;
- are not writable by group or others;
- are not symbolic links.

The root's parent must also be either sticky (as `/tmp` is) or owned by the
current user or root and writable by no one else. Otherwise another user could
rename the root away and substitute their own tree. The build root itself is
canonicalized when selected, so a symlinked `--build-dir` works.

Otherwise the entry is rejected and the module is compiled from Wasm.

The current user is determined by creating a file in `tmp/`. Wasmtime then
verifies the serialized header's version and configuration.

On other platforms, the build directory's own access control is the boundary.
Treat the build directory like the compiler binary: do not share it with
untrusted users.

Wasm-level entries (fragments and build outputs) are not native code. They are
validated structurally and by identity before use.

## Budgets and accounting

Cache work never refills or extends a compilation budget:

- **Native compilation:** never charged to Suss fuel, cached or not.
- **Fragment lookups:** charged exactly as before. Key formation and IR
  verification are debited from the caller's artifact-preparation budget. A
  disk read is host I/O and is not charged. Every entry read is bounded at
  1 GiB; larger files are rejected unread.
- **Whole-build validation:** host work that runs before any compilation budget
  starts. It rereads the recorded inputs.

`--compile-fuel` is part of the whole-build key. A hit therefore never succeeds
where recompiling with the same budget would exhaust it, and
deliberate-exhaustion tests still exercise the real pipeline.

A whole-build hit runs no compiler, so compiler fuel traces
(`SUSS_MACRO_FUEL_TRACE`, `SUSS_METADATA_FUEL_TRACE`) print nothing. Use
`--no-cache` when calibrating compilation fuel.

## Retention

After its first write, a process prunes the cache to three quarters of
`SUSS_CACHE_MAX_BYTES` (default 4 GiB), unless any process sharing the
directory pruned it within the last ten minutes (`.last-prune`). It removes the
least recently used entries; a hit refreshes an entry's modification time.

This limit concerns disk use only. It is independent of language memory limits
and of the bounded in-memory caches.
