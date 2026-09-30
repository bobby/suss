# Working on Suss

Read [the accepted design](docs/design/suss-0.3.1.md), [ROADMAP](ROADMAP.md),
[compatibility inventory](docs/compatibility/README.md) and
[handoff](docs/roadmap/handoff.md) before changing architecture.

* Implement the ClojureScript portable contract, not JVM Clojure semantics.
* The current compiler is a prototype. Do not infer support from old comments or
  an encoding-only test; validate and execute the relevant artifact.
* Keep behavior, tests, inventory and evidence aligned. Never replace unknown
  results with success, hide failures with blanket skips, or claim milestones
  complete without their acceptance tests.
* Add focused regressions for semantic bugs. Preserve source evaluation order.
* Record source provenance and upstream licenses when porting core forms.
* Do not set RUSTFLAGS in commands; the developer environment manages it.
* Use `cargo test --workspace --locked -- --test-threads=2` for the full baseline.
  Run focused tests first. Shared Wasmtime engines and lean test profiles keep
  local resource use bounded. Do not delete unrelated files to make disk space.
* Link PRs to issues: use `Closes #N` only when all issue acceptance criteria
  are fulfilled; use `Refs #N` for partial progress. Dispatch an independent
  subagent review for each PR, push significant fixes and require final-head CI.
  Do not merge PRs without a later explicit user instruction.
* No Java dependency in shipped code. JVM/Node may be development test oracles.
* Update docs/roadmap/handoff.md with commands, results, limitations and the next
  unblocked task. Keep roadmap issue IDs stable. Do not mark future work complete.

Existing crates: `suss-core` (forms), `suss-reader`, `suss-compile`, `suss-cli`.
There is no `suss-eval` crate. `core.sus` is the active prototype library.
