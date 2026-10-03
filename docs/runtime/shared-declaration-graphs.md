# Shared declaration and reader data graphs

The retained core namespace previously failed before a compiled macro could read
its declaration facts: construction exceeded the 65,536-node analysis graph
limit. Namespace snapshots copied unchanged declaration revisions, and overlapping
reader trees and environment records repeatedly constructed the same data.

Compiler environments now own immutable `Arc<DefinitionInfo>` revisions.
Snapshots and global source facts share unchanged revisions; reanalysis installs
a new revision. Older snapshots preserve their original definition, initializer
and `defonce` facts. Per-build graph memo tables retain their identity owners.
No address becomes a persisted artifact key.

Canonical reader data uses shared subtree recipes. Exact tagged, framed keys
preserve binary64 bits, UTF-16 units, namespace/name distinctions, metadata and
entry order. Bare compiler byte spans are kept in compiler records; the existing
native form transport carries source positions through reader metadata. Data
sharing preserves those metadata values. Ordered maps/vectors, primitive values
and immutable environment records also share identical data within the build.
Compiled macro bodies still execute for every invocation.

The graph retains its 65,536-node and 1,048,576-UTF-16-unit limits. Independent
per-form validation preserves 4,096 logical transport nodes, including normalized
metadata, and 64 reader levels. Stored reader keys additionally have a 32 MiB
byte limit. Limits are checked before graph materialization or macro invocation.
No exported field is omitted to fit the core graph.

The actual full core-namespace declaration projection passes in both caller
phases, before and after a new `defonce`. Snapshot identity/revision tests pass.
Canonical transport parity after GC covers collection metadata, source locations,
lone surrogates, signed zero and NaN payloads. The broader affected run passes
73 tests; final bounds/graph/core checks pass 23 tests. Bootstrap manifests have
the updated compiler identity; both Wasm images remain byte-identical.
Commands, unsuccessful intermediate attempts and logs are in the handoff.

Full workspace baseline, independent PR review and exact final-head CI remain
required. This resolves a bounded construction prerequisite for issue #14;
complete portable schema, source/macro artifact cache keys, temporary evaluator
removal and M3 lifecycle acceptance remain open.
