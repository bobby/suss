# ADR-0002: Defer MLIR compiler adoption after the bounded evaluation

Status: **Accepted — defer adoption**.
Date: 2026-10-10.
Issue: [#188](https://github.com/bobby/suss/issues/188).

## Context

The bounded #188 investigation asked whether MLIR could reduce compiler
maintenance while preserving the accepted portable ClojureScript contract and
the shared WasmGC runtime ABI. It evaluated an isolated registered Suss dialect,
a verified graph export and Rust bridge, using the existing emitter and runtime.
It did not migrate or replace the shipped compiler.

The final executed snapshot was PR #227 head
[`b82ad94`](https://github.com/bobby/suss/commit/b82ad94cf7545a4f80315af3a6f677e57fc4bd13),
later merged as [`f4b13f3`](https://github.com/bobby/suss/commit/f4b13f306ad354c6cbe5bc0c49cc44975249396d).
The post-merge evidence audit and completed decision record were published by
[PR #229](https://github.com/bobby/suss/pull/229). The audit distinguishes the
executed snapshot from later documentation and compiler heads; its receipts do
not claim that later source changes were executed by the spike.

## Decision

Keep the existing Rust frontend/IR, lowering, WasmGC emitter and Wasmtime runtime
as the shipped compilation path. Defer MLIR adoption. Retain the experiment and
its evidence as a bounded evaluation; do not add MLIR or LLVM as a shipped
compiler dependency or claim a compiler migration.

The evaluated route—local Suss dialect, local verified graph export, local Rust
IR reconstruction, then the existing WasmGC emitter—demonstrated useful
verification infrastructure, but did not demonstrate lower ongoing maintenance
cost. The slice retains substantial local source-schema checks, ABI adaptation,
effect/cell conventions, Rust reconstruction and the existing backend. The
hand-authored graph is not a general source-to-MLIR compiler. WasmSSA in the
inspected pinned SDK lacked the required GC reference/recursive type, typed call
and exception operations; a direct route would require implementing and
maintaining those capabilities. The tested SDK/exporter setup was macOS ARM64;
cross-platform distribution was not established.

## Evidence and limits

The post-#228 audit maps each #188 criterion to retained commands, logs and
limitations in [`experiments/mlir/evidence/post228/README.md`](../../experiments/mlir/evidence/post228/README.md)
and [`audit.json`](../../experiments/mlir/evidence/post228/audit.json). It records:

- Nine successful execution gates, including actual WasmGC execution through
  pinned Wasmtime, shared recursive GC types across fragments, a rooted captured
  closure after forced GC, and the ABI compatibility gate.
- Genuine source facts, binding identity, metadata and macro-visible structure
  preserved through the representative lowering; seven effectful examples with
  ordered effects and cleanup; and 92 paired fragment executions matching exact
  result bits after GC.
- A completed assessment of the WasmSSA route and local implementation boundary.
- Paired measurements, raw samples and artifact sizes, with excluded costs and
  host/process differences documented. They do not establish production latency,
  equivalent-feature size, build-time or ongoing-maintenance superiority.
- The required full workspace baseline on the audited documentation head:
  1,550 passed, zero failed, 41 existing ignored; all ten final-head CI checks
  passed. The historical exact execution identity and its validation receipts
  are recorded in the post-#228 evidence package and PR #229.

This evidence establishes the bounded evaluation's decision, not general MLIR
support, scheduler completeness, cross-platform release readiness, or future
compiler-head correctness. The accepted portable-language contract and M4
collection acceptance criteria are unchanged.

## Consequences

The current compiler path remains the implementation baseline. The isolated
experiment remains available for future comparison without creating a second
production pipeline. No new implementation work is authorized by this ADR.

Reconsider MLIR only if a concrete maintenance problem and a measurable benefit
are identified. Any adoption proposal must be a separate reviewed architectural
decision with a source-to-operation mapping contract, equivalent semantic
verification, reproducible supported-platform builds, paired full-cost
measurements, and a migration/removal plan for duplicated validators and paths.
The smallest candidate boundary is an optional analysis/verification adapter
over genuine immutable source facts, with the current IR, emitter and runtime
retained until equivalence and value are demonstrated.

## References

- [Accepted Suss design](../design/suss-0.3.1.md), especially sections 5–7 and 9.
- [MLIR evaluation decision and route assessment](../../experiments/mlir/decision.md).
- [Post-#228 evidence audit](../../experiments/mlir/evidence/post228/README.md).
- [Merged implementation and evaluation PR #227](https://github.com/bobby/suss/pull/227).
- [Merged evidence audit PR #229](https://github.com/bobby/suss/pull/229).
