# Resurrection roadmap

The accepted target is [the design specification](docs/design/suss-0.3.1.md).
The prototype has not reached the WASI alpha gate. Work below is dependency ordered;
status is evidence-based and is not a calendar promise. See the [handoff](docs/roadmap/handoff.md).

| Milestone | Depends on | Exit gate |
| --- | --- | --- |
| M0: Contract and feasibility | — | All M0 acceptance criteria pass |
| M1: Trustworthy evidence | M0 | All M1 acceptance criteria pass |
| M2: Compiler and runtime foundation | M0, M1 | All M2 acceptance criteria pass |
| M3: Persistent development environment | M2 | All M3 acceptance criteria pass |
| M4: Portable persistent collections | M2 | All M4 acceptance criteria pass |
| M5: Generic WIT interoperability | M2, M4 | All M5 acceptance criteria pass |
| M6: WASI 0.3.1 alpha | M3, M4, M5 | All M6 acceptance criteria pass |
| M7: Portable compatibility beta | M6 | All M7 acceptance criteria pass |
| M8: Browser beta | M6 | All M8 acceptance criteria pass |
| M9: CSP extension | M6, M7 | All M9 acceptance criteria pass |

M4 and M5 can overlap after their foundations; M8 feasibility happens in M0,
with browser product delivery after the WASI alpha. CSP is deliberately last.

## Work packages

[Machine-readable issues](docs/roadmap/issues.json) contain stable IDs, dependencies,
objectives, acceptance criteria, scope and proposed validation commands. Commands
for planned suites are **acceptance targets**, not existing runnable scripts.
Published: [10 milestones](https://github.com/bobby/suss/milestones) and
[39 issues](https://github.com/bobby/suss/issues). Stable IDs map to remote URLs in
[github.json](docs/roadmap/github.json). The publisher creates missing entries and
preserves existing issue bodies.

### M0: Contract and feasibility

- **M0-01 — Contract and upstream inventory** (in-progress). Record the accepted design, pin ClojureScript and inventory core declarations without guessing portability.
- **M0-02 — Lock toolchain and official WIT packages** (in-progress). Select a tested Wasmtime/compiler-tools/WIT package set for WASI 0.3.1. Official source graph is locked; Rust migration and executing boundary probes remain.
- **M0-03 — Prove shared GC fragments** (in-progress). Build a minimal shared-runtime plus two separately compiled fragments before replacing the REPL.
- **M0-04 — Prove browser loading and suspension** (in-progress). Load a minimal core GC module through ES modules and bridge one asynchronous operation.

### M1: Trustworthy evidence

- **M1-01 — Strict conformance decoding and baseline** (in-progress). Replace wildcard GC results and non-failing conformance reports with independent decoding and explicit failures.
- **M1-02 — ClojureScript oracle and semantic regressions** (in-progress). Build a pinned development-only oracle and lossless values/effects corpus.
- **M1-03 — Bounded CI and reproducible baseline** (in-progress). Bound test resource use and make baseline results reproducible.

### M2: Compiler and runtime foundation

- **M2-01 — Reader forms, metadata and namespace phases** (planned). Introduce source spans, retained metadata and deterministic namespace/reader conditional resolution.
- **M2-02 — Explicit evaluation-order IR** (in-progress). Lower effectful operands into explicit temporaries and blocks.
- **M2-03 — Runtime ABI v1 and closures** (planned). Implement stable GC prelude, f64 numbers, UTF-16 strings, closure ABI and binding cells.
- **M2-04 — Nominal types, protocols and exceptions** (planned). Introduce descriptor identity and protocol/exception machinery on the stable ABI.

### M3: Persistent development environment

- **M3-01 — Incremental compiled REPL** (planned). Replace source replay with one runtime and compiled input fragments.
- **M3-02 — Namespace loading and redefinition** (planned). Implement live binding cells, namespace loading, defonce and reload semantics.
- **M3-03 — Compiled macro bootstrap** (planned). Run macros in a separate compiled phase session and remove the temporary evaluator.
- **M3-04 — Session lifecycle and interruption** (planned). Define reset, roots, code residency and cancellation while interactive I/O is pending.

### M4: Portable persistent collections

- **M4-01 — Upstream extraction and adaptation provenance** (planned). Create reproducible form extraction and patching for reviewed core definitions.
- **M4-02 — Sequences, lists and vectors** (planned). Port collection foundations, lazy/chunked sequences, vector/subvector and map entries.
- **M4-03 — Maps, sets, queues, records and sorted types** (planned). Port HAMTs, sorted collections, queues and record behavior.
- **M4-04 — Hashing, metadata, transients and reduction** (planned). Complete shared collection protocols and all reduction paths.

### M5: Generic WIT interoperability

- **M5-01 — Generate bindings from resolved WIT** (in-progress). Replace hardcoded WASI names with selected-world binding generation.
- **M5-02 — Resource ownership and scopes** (planned). Implement constructors, methods, statics, own/borrow and explicit close.
- **M5-03 — Canonical memory allocation and cleanup** (planned). Implement checked realloc/free, post-return and async transfer lifetimes.
- **M5-04 — Bidirectional Rust interoperability fixtures** (planned). Generate an independent Rust host/guest corpus for all WIT boundary shapes.

### M6: WASI 0.3.1 alpha

- **M6-01 — Continuation scheduler and future API** (planned). Implement future/await with GC continuation state machines and a cooperative scheduler.
- **M6-02 — Canonical async, futures and streams** (planned). Connect continuations to canonical async imports/exports and future/stream values.
- **M6-03 — Complete official WASI capability bindings** (planned). Generate and exercise the entire pinned WASI 0.3.1 package graph.
- **M6-04 — CLI commands, HTTP applications and interactive I/O** (planned). Deliver command/library workflows and alpha release gate.

### M7: Portable compatibility beta

- **M7-01 — Finish portable public core and macros** (planned). Review and implement every remaining portable inventory item.
- **M7-02 — Sequence and transducer behavior** (planned). Complete higher-order arities, transducers, chunking and laziness effects.
- **M7-03 — State, delays, multimethods and printing** (planned). Complete atoms/watches/validators/CAS, volatiles, delays, multimethods and printer contracts.
- **M7-04 — Compatibility and migration release report** (planned). Publish evidence and retire superseded prototype paths.

### M8: Browser beta

- **M8-01 — ES module and declaration packaging** (planned). Produce distributable browser modules using the validated M0 path.
- **M8-02 — DOM, fetch, events and Promise bridge** (planned). Implement typed browser host APIs and shared async semantics.
- **M8-03 — Browser application and library examples** (planned). Ship examples demonstrating both browser usage modes.
- **M8-04 — Cross-browser corpus and distribution** (planned). Run the shared compatibility corpus in declared browser versions.

### M9: CSP extension

- **M9-01 — Channels, buffers and selection** (planned). Implement CSP channels over the established scheduler.
- **M9-02 — go state machines** (planned). Implement core.async-style go lowering using continuation infrastructure.
- **M9-03 — Future and stream channel adapters** (planned). Provide explicit adapters between CSP and component async values.
- **M9-04 — core.async compatibility corpus** (planned). Port selected core.async tests with documented scope.

## Completion discipline

Every implementation session adds a failing regression, makes the smallest coherent
change, executes its relevant checks, and updates evidence/handoff. A package
is complete only when its acceptance criteria pass. A known-failure baseline
does not certify compatibility. Toolchain limitations remain explicit blockers,
not reasons to silently weaken the contract.
