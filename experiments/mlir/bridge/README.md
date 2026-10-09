# Bounded MLIR → portable IR → actual WasmGC bridge

The isolated bridge builds against the locked dependency graph. On main
`22723b432c5fb4792c39302d3739b73a44fa87ff`, all 18 unit tests pass; actual
original/mutated source-v2 execution produces exact binary64 9/13 and all seven
effects result/ordered-cell cases pass. The refreshed paired measurement passed all 92 executions. Full-baseline, publication and final-head CI gates remain pending.
The experiment retains the existing production compiler and runtime.

Historical failed unit runs and pre-rebase measurements are retained in the
[main evidence record](../README.md) and [decision](../decision.md). The former
source/global failure is repaired; it must not be reported as the current result.
Current-base logs are under [evidence](../evidence/).

## Export and execution contract

The numeric [schema](schema.json) specifies syntax; Rust additionally enforces
semantic checks. C++ verifies registered MLIR operations before export. The
consumer reconstructs the exported graph through public IR and `compile_ir`;
it executes that graph rather than substituting original source or reference HIR.
`verified_mlir: true` is a producer attestation, not independent authentication.
Retain verified input, exporter diagnostics and output identities with execution
results. Unknown fields, duplicate keys, forward references, implicit captures,
unsupported operations and regions reject rather than silently disappearing.

A numeric graph has one block. Parameter indexes precede serialized operation
indexes; each operation produces one result, and `return_value` selects the final
return operand. V1 omits locations/source facts and rejects source carriers,
including explicit null. The separate [v2 source profile](source-schema.md)
preserves complete genuine facts and uses independent reanalysis plus bounded
representative identity/operand correspondence. It does not establish arbitrary
source lowering or operation-level diagnostic mapping. The [effects profile](effects-schema.md) separately defines ordered cells and structured cleanup.

Numeric execution compiles independent producer/caller fragments with shared
recursive runtime types, publishes a captured closure in a rooted binding cell,
forces GC, invokes through that cell, forces GC again and independently decodes
the returned f64 GC struct. Source-v2 adds a custom section to both fragments,
reseals artifact identity, independently reads it back and validates identity/ABI
before instantiation. Validation, linkage, traps and decode errors propagate.

The bounded initializer probe rejects incompatible ABI/compiler manifests before
a synthetic counter effect and requires a compatible control to run exactly once.
It supplements actual graph artifact validation; it is not an exported MLIR
initializer. All seven effects graphs execute in Wasmtime with exact result bits
and ordered journal/count observations, including cleanup overriding both success
and a handled exception. These are synchronous cases, not a scheduler proof.

## Reproduction

`Cargo.lock` pins transitive dependencies in this separate workspace. The isolated
profiles disable debug information/incremental compilation and retain assertions.
Use one Cargo graph at a time, with two build jobs; do not share this target with
another worktree's path crates. No Java dependency is shipped.

```sh
CARGO_BUILD_JOBS=2 cargo test --manifest-path experiments/mlir/bridge/Cargo.toml --locked --offline -- --test-threads=2
CARGO_BUILD_JOBS=2 cargo build --manifest-path experiments/mlir/bridge/Cargo.toml --locked --offline
python3 experiments/mlir/bridge/source-fixtures/check-source-export.py BRIDGE EXPORTER --execute
python3 experiments/mlir/bridge/effects-fixtures/check-native.py BRIDGE
python3 experiments/mlir/scripts/measure-pipelines.py BRIDGE EXPORTER
```

Offline reproduction requires the pinned dependency cache. LLVM acquisition and
C++ build instructions are in the parent README/toolchain lock. The analyzer
accepts a UTF-8 source file bounded to 1 MiB, uses the real validated bootstrap
catalog, selects a genuine source child and propagates unsupported forms/errors.
It emits source facts, not an executable replacement for the exported graph.

The bridge uses the repository's public compilation, artifact identity, runtime
ABI and Wasmtime patterns. It is original experiment code; no upstream language
implementation is ported here. LLVM is a development dependency under Apache-2.0
WITH LLVM-exception. Compiler adoption requires a separate architectural decision.
