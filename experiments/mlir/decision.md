# MLIR evaluation recommendation — #188

Draft evidence record, 2026-10-08. Recommend **deferring compiler adoption**.
This is an isolated investigation, not an architectural decision or authorization
to migrate the shipped compiler. Earlier rebuilding on main `22723b4` passed all
18 unit tests, actual v2 source preservation/correspondence, seven effects cases
and 92 paired executions. The evidence and measurements below refer to that
retained snapshot unless explicitly labeled post-#226. Fresh rebuilding and
execution on the rebased `4e0f00d` checkout now pass: 18 Rust tests, genuine
source-v2 preservation, seven native/pinned effects cases, 92 paired executions,
and pinned/CLI source observations. Post-#226 C++ gates also pass. See
[evidence receipts](evidence/post226-execution-gates.json) and the fresh content
identity manifest. Independent review at `db79427` found no material issues;
full baseline and final-head CI remain pending.

The custom dialect provides useful registered types, region structure and
verifier hooks. This slice still implements the language-specific source-facts
validator, closure ABI adaptation, effect/cell conventions and JSON-to-Rust-IR
bridge locally. It delegates final WasmGC emission to the existing backend and
native code generation to the existing Wasmtime. No measured reduction in
ongoing maintenance has been established.

## Evidence and limits

| Requirement | Current evidence | Limit / next gate |
| --- | --- | --- |
| Actual WasmGC and shared closure | Executed numeric original9/mutation13, two independent fragments, rooted binding cell, forced GC, pinned Wasmtime49.0.1 | Bounded numeric graph, not complete source compiler |
| ABI compatibility | Incompatible ABI/compiler manifests reject before a synthetic initializer marker; compatible control runs once | Synthetic initializer; actual fragment manifest and integrity validation also required |
| Original source analysis | Genuine native facts survive complete registered MLIR parse/print twice, including source forms/children, lexical and physical identities, metadata and spans | Actual v2 section preservation/resealing passes before-main; current-base execution passed; final gates pending |
| Source observations | Fresh pinned CLJS gives exact binary64 9/13 for the identical original/mutated closure source; existing Suss CLI prints 9/13 | CLI printed scalars are separate from the bridge's independent boxed-value decoder; graph is hand-authored; bounded identity/operand correspondence now checked before-main, current-base passed; strict-type repair independently reviewed |
| Ordered effects and cleanup | Seven actual exported MLIR graphs match result and ordered cell snapshots, plus fresh pinned CLJS observations | Supports these closed profiles; no general effects or scheduler claim |
| Same-example comparison | Native producer/caller source and verified MLIR represent the same capture, arithmetic and live-cell call; strict execution/measurement driver authored | Actual92 executions and timed emission pass; current-base execution passed; final gates pending; source-to-MLIR conversion absent |

Original commands and limitations live in [README](README.md),
[source profile](bridge/source-schema.md), [effects profile](bridge/effects-schema.md)
and the handoff. Portable current-main logs, command/status records and identity manifests are
retained under [evidence](evidence/); [reproduction](reproduce.md) covers clean setup.

## Route assessment

The [documented WasmSSA dialect](https://mlir.llvm.org/docs/Dialects/WasmSSAOps/)
was inspected on 2026-10-08, separately from the pinned LLVM23.1.3 SDK. The SDK's
`WasmSSATypes.td` admits numeric/vector values and opaque `funcref`/`externref`;
`WasmSSA_FuncCallOp` takes a flat symbol reference. In the inspected pinned
definitions there are no GC struct/array/i31/eqref types, recursive GC type-group
definitions, typed closure `call_ref`, or language-EH operations. Executed direct
probes accept the numeric constant and reject the tested eqref, struct.new and
throw spellings; [observations](feature-probes/observations.json) retain errors
and the operation-source hash. These observations identify the required additions
for this route, not an impossibility result about MLIR or future upstream work.

The working route is a **local Suss dialect → local verified graph export → local
Rust IR reconstruction → existing WasmGC emitter**. A direct WasmSSA route would
need GC/recursive-reference/typed-call/EH definitions, lowering and emission support,
plus runtime ABI integration and tests. Lowering through ordinary LLVM linear-memory
Wasm would require a separate proof of the accepted GC ABI and does not satisfy
it merely by producing a valid numeric Wasm module. No direct LLVM route was
implemented in this evaluation.

## Maintenance, diagnostics and distribution

`scripts/measure-implementation.py` records per-file SHA256, bytes and physical/
nonblank lines, excluding generated code and dependencies. The footprint is an executing per-file report, not a maintenance saving. Its
counts must be refreshed after final source formatting; dense C++ formatting and
comments affect physical lines. Reused native groups support much more behavior
and remain required. They are not equivalent replacement costs.

The [current-main comparison](evidence/pipeline-comparison-current-main.json)
executes 92 fragment pairs and independently decodes 9/13 after GC/ABI gates.
Compilation/emission medians are 6.087/6.137ms for native original/mutated source,
versus 19.432/19.420ms for hand-authored MLIR export plus Rust reconstruction/emission.
The earlier before-main report remains available as historical evidence.
Native producer/caller artifacts are1036/910bytes; MLIR940/845bytes. Native source
identity annotations differ from IR annotations. Timings include launches/file
writes and exclude Wasmtime execution, builds, source-to-MLIR conversion and v2
sidecars. MLIR uses two processes versus one native process; shared-host scheduling
is not isolated. No production-performance or faster-compiler claim follows.
The source code of native production crates is unchanged by the rebase, but
library data changed; the rebuilt current-base binary and full comparison now
pass. Final publication gates remain required. Raw samples, platform and binary/fixture/artifact hashes are retained.

MLIR verifies registered operation types, captures, arities, region ownership and
source-schema consistency; actual negative fixtures retain diagnostics and
unchanged-output sentinels. Rust still verifies reconstructed IR and validates
Wasm and runtime/artifact identities. Initial genuine-source failures showed
that synthetic shape checks did not prove compatibility: parameter HIR IDs can
remap while lexical identities survive. The repair retains both identities and
rejects outer-binding remaps and stale/conflicting facts. Source v2 also exposed
the need to reseal after adding custom data. This is one-time integration cost
and evidence against assuming generic infrastructure eliminates semantic work.

Reproducibility pins the LLVM release commit/archive, Rust lockfile, Wasmtime and
wasm-tools dependencies. The tested SDK is macOS ARM64 only and occupies7.6GiB;
the exporter links system libSystem/libz/libc++, with no SDK dylib dependency in
the inspected build. Exporter size/latency from the earlier v1 build is historical
stage evidence and must be remeasured for the current binary. LLVM uses
Apache-2.0 WITH LLVM-exception; no LLVM/exporter binary is shipped. Linux and
release distribution are not proven.


The current [per-file footprint](evidence/implementation-footprint.json) records
source identities and these totals. Regenerate it after any implementation edit.

| Component | Files | Physical lines | UTF-8 bytes |
| --- | ---: | ---: | ---: |
| additional handwritten mlir cpp tablegen | 9 | 887 | 61322 |
| additional handwritten bridge rust | 6 | 2576 | 103011 |
| additional experiment python | 19 | 1140 | 70497 |
| reused native portable frontend ir emitter | 50 | 24641 | 917762 |
| reused native runtime abi | 30 | 14824 | 413015 |

| Concern | Existing native route | Evaluated MLIR route |
| --- | --- | --- |
| Semantic ownership | Reader, resolver, source-aware HIR, lowering and runtime ABI remain in Rust | Reuses those components and adds C++ source-schema validation plus graph export and Rust reconstruction; removing either validator requires a separate equivalence proof |
| Diagnostics | Source forms and spans remain available through native analysis | Registered operation failures identify MLIR operations; module witness preserves source facts, but source-to-operation diagnostic mapping is still unproven |
| Verifier coverage | Native IR, Wasm validation, artifact integrity and runtime ABI checks | Adds registered type/region/capture/arity checks and strict source/effect profiles; retains every native execution gate; profile rejection does not establish arbitrary source support |
| Build cost | Existing locked Rust dependency graph | Additional pinned LLVM SDK, TableGen and C++ tool build; emission timing excludes these builds, so no comparative build-time claim is supported |
| Distribution | Existing compiler/runtime packaging | Tested macOS ARM64 exporter only; no Linux/release packaging proof, and the SDK is a development dependency rather than a shipped runtime requirement |

The current exporter is 5,002,288 bytes (SHA256
`adc5515abca4a24bb68687815bc457cb3134cefb474696b35c6173aed9fd3d0c`).
A fresh `otool -L` inspection shows only system libSystem, libz and libc++.
This confirms the tested executable's dynamic dependencies; it does not measure
clean build time or prove portability to other platforms.

## Continuations and smallest future boundary

The effects examples are synchronous. Rooted stackless suspension would require
continuation GC layouts shared across fragments, explicit resume states, captured
locals/handlers/dynamic frames, at-most-once resume and cancellation checks,
exceptional cleanup across suspension, ownership of pending operations, and
reentrancy/race tests. This evaluation provides no complete scheduler and does
not advance M5–M7 acceptance.

If future evidence shows a concrete maintenance benefit, the smallest proposed
boundary is an optional analysis/verification adapter consuming genuine immutable
source facts alongside the existing IR, with the current emitter/runtime retained.
Before adoption, require a separately reviewed architectural decision, a source-
to-operation mapping contract, equivalent full semantic negatives, reproducible
cross-platform builds and measurements, and an implementation/removal plan for
duplicated validators. Deferral keeps this evidence reusable without committing
the compiler to an unproven migration.
