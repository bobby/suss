# Indexed source provenance

SourceOrigin verifies original syntax before adding reader location metadata to
compiled macro forms. Its immutable source snapshot now indexes source spans,
so each query compares only forms with the requested span rather than walking
the entire original and located syntax trees again. Paths refer to the existing
owned forms; indexes do not clone subtrees or hold references into movable storage.
Cloned origins share the snapshot and its cached construction errors.

The index retains the linear matcher’s first-match order: source roots in order,
then reverse code children followed by reverse metadata children. Matching still
checks complete syntax, explicit metadata where required, and exact number bits.
A source span alone never establishes provenance for generated syntax. Logical
visit ranks charge the same shared traversal budget as the previous matcher,
including exhaustion before a candidate and across original/located queries.
Only the first 1,048,576 reachable nodes are indexed. An early verified match
remains usable when later nodes exceed that prefix. The existing 1 MiB source
limit, malformed-source diagnostics and Unicode-path requirement are unchanged.
Reader-expansion correspondence remains on its existing paired traversal.

Five executing unit regressions compare against the unchanged test-only linear
matcher. They check first-match pointer identity, remaining budgets and error
locations/messages, reader metadata, equal spans, NaN payloads and signed zero,
truncated prefixes, empty trees and shared snapshot ownership. A late unique
span in 10,000 forms has one candidate while retaining its 10,000-visit charge.
The original three declaration-metadata and ten source-metadata tests execute
unchanged, including generated syntax, location overrides, source provenance,
bounded failure recovery and rejection of invalid source/non-Unicode paths.

## Executed evidence

Commands use installed RUSTUP_TOOLCHAIN=1.98.0, the shared lean target directory
and CARGO_BUILD_JOBS=2; RUSTFLAGS is not set.

```sh
cargo test -p suss-compile --lib provenance_index --locked -- --test-threads=2
cargo run --profile test -p suss-cli --bin suss-bootstrap --locked -- runtime/bootstrap
cargo test -p suss-cli --locked --test compiled_macro_declaration_metadata --test compiled_macro_form_source_metadata -- --test-threads=2
sh scripts/verify-bootstrap.sh
```

The five focused unit tests pass (98 unrelated unit tests filtered); declaration3
and source-metadata10 pass with no ignored or filtered tests. Both phase Wasm/JSON
pairs regenerate and reproduce twice byte-exact with Java and Node absent; build
identity/invalidation checks and all four executing bootstrap tests pass.

The unchanged declaration suite took 360.82 seconds here, compared with 943.63
seconds in the immediately preceding frozen b210bbb full run and 501.21 seconds
in an earlier parent run. These are separate local observations, not a stable
benchmark or a timing acceptance threshold. A one-second read-only sample before
the change showed original-form scanning; the later sample showed analysis-graph
materialization instead. That remaining work is not optimized in this change.

Independent subagent review, the required full workspace baseline and final-head
CI remain required before readiness. This does not establish complete source AST
schemas, production evaluator retirement, scheduler/cancellation behavior or any
original [M3 acceptance gate](../roadmap/acceptance-m3.md).
