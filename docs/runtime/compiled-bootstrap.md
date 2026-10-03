# Versioned compiled bootstrap

Fresh runtime and macro Sessions now install phase-specific shipped WebAssembly
images for the retained portable core profile. Each Store executes its own image
initializers once; immutable bytes and compiler catalog facts are cached, while
guest roots, bindings, values and initialized state are never shared. Reset creates
and provisions a replacement Store. Input evaluation does not replay core source.

Source catalog restoration uses the ordinary bounded reader and analysis path,
without emitting Wasm or evaluating source. It preserves the source declaration,
callable, namespace and position facts needed by compiled macro environments.
The existing bounded bootstrap forms break the compiler/core cycle. This is the
selected imported profile, not complete public core compatibility.

Each manifest records format and phase, core source SHA256, compiler/reader/core
implementation SHA256, locked compiler/runtime ABI, Wasm tooling, dependency graph
SHA256, target, compiler flags, artifact SHA256 and canonical binding identities.
The current bounded bootstrap has no configurable compiler options, so its flags
list is empty. Build-time source identity hashes sorted repository-relative paths
and contents with length framing; checkout location is excluded. Changed identity,
missing/unknown manifest fields, corrupted bytes and incompatible phase are
rejected before installation. Hosts still validate and link the actual Wasm.

Regenerate after compiler, ABI, dependency or selected source changes:

```sh
CARGO_BUILD_JOBS=2 cargo run --profile test -p suss-cli --bin suss-bootstrap --locked -- runtime/bootstrap
scripts/verify-bootstrap.sh
```

The verifier supplies Cargo/linker tools through a restricted PATH where Java and
Node are unavailable. Two fresh generator processes must produce identical Wasm
and manifests, matching the shipped files. Native tests then execute source
macros with syntax quote in both caller phases, force GC, test Store/reset
isolation and reject every changed manifest identity field and corrupted bytes.
All four tests pass. The runtime image is1,103,137bytes and the macro image
1,102,129bytes; both generated from the retained149,419-byte source. This proves
the bounded bootstrap path runs without Java/Node available; the development
primary-oracle fixtures retain their separate JVM/Node workflow.

The initial focused image test failed on the intentionally empty manifest before
real generation. The generated images then pass exact regeneration/restoration
and execution. Affected validation passes66 compiler tests and83 tests across eight native
REPL, phase, macro, metadata and live-cell targets, with no failures or ignores.
The final workspace baseline, independent PR review and exact-head CI remain
publication gates.

A complete cache for user macro dependencies and executing changed-macro/unchanged
cache tests remains unfinished. The temporary evaluator in the legacy compiler
also remains until replacement acceptance permits removal. Complete portable
macro environment schema, namespace policy and lifecycle cancellation/live-memory
accounting remain M3 work. This artifact slice closes neither issue#14 nor M3.
