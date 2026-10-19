# Source artifact cache

Native compiled macro hosts retain a bounded process-local cache of emitted Wasm.
Analysis and compiled macro execution still run for every input, before lookup.
Every preparation therefore returns fresh staged environment and cell facts;
execution still initializes the requested input once. The cache cannot suppress
compile-time effects, runtime effects or namespace checks.

The key includes exact source text and path when available, selected reader forms,
phase, complete compiler-source fingerprint, runtime ABI identity, the fixed
portable GC target/default profile/empty flags, immutable loaded macro-module
source identities and published macro-declaration source identities. It also
includes the actual lowered IR, with binary64 payload bits recorded separately.
Changing dependency source invalidates reuse even if the resulting expansion is
identical. File edits alone do not change identities of already-loaded modules;
explicit reload selects new immutable snapshots. Dependencies include their
canonical namespace edges and source paths. Declaration rollback restores only
its own provenance, preserving successfully loaded dependencies. Discovery failures
before execution retain the known graph. Selected modules are marked incomplete
before execution; if initialization fails after publishing effects or declarations,
all emission reuse bypasses until successful loading establishes every selected
module identity. Actual IR alone does not establish unknown graph provenance.

IR verification precedes every lookup. A hit verifies the retained Wasm digest
and runtime ABI before installation; the ordinary host validation/linking gates
remain mandatory. Unknown external expansion-host provenance bypasses reuse.
The internal IR Debug representation is qualified by compiler identity and is
only a process-local key encoding, never a portable serialized artifact format.

Optional key construction has a 64K charged-work admission allowance across source
text, reader forms, dependency comparisons, IR Debug encoding and numeric-bit
visits. Full IR verification runs before this allowance applies. Each attempted
operation first debits the caller's shared budget; real fuel and cancellation
errors propagate even on the operation crossing admission. Decline discards the
partial key and emits the fully verified IR fresh, without looking up, inserting
or evicting entries. It imposes no source-size rejection or new operation fuel.
Completed small keys retain their encoding and total charged work. Cache counters
include these successful fresh emissions as bypasses.

Admitted key work is capped at 64K; the crossing attempt is additionally billed
in full, and a produced Debug fragment may exceed that allowance by itself.
This is not a byte-size or wall-clock bound. Formatting, framing and
hashing overhead and the existing fresh-emission/cached-byte validation and copy
accounting gaps remain separately documented in
[computation budgets](macro-computation-budgets.md).

Each host retains at most64 entries and32MiB of Wasm, evicting oldest unused
entries first. Oversized artifacts execute without being retained. Reset creates
a fresh host/cache. Cache counters describe artifact reuse, not resident code or
live GC objects; the existing native code cache remains separate.

Focused evidence: four compiler checks passed for source/dependency/phase/path
keys, unknown provenance and verifier failures, NaN payload distinction and
corrupt-entry rejection, and bounded eviction. Four native executing tests
passed for observable macro/runtime effects on a cache hit, reload-all through
changed transitive dependencies and old captures, and declaration provenance
restoration after a script compile error, and actual distinct NaN payloads
through cached execution with unknown-provenance bypass.

This is an emitted-artifact cache, not an analysis/expansion cache. Complete
published user artifact manifests, component/AOT migration, temporary evaluator
retirement and lifecycle/scheduler acceptance remain open M3 work. No original
M3 issue closes from these focused results. Independent review, required full
baseline and final-head CI remain pending.
