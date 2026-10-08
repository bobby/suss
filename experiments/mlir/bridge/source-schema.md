# Explicit source-preserving numeric experiment

`suss.mlir.bridge.source.v2` supplements the unchanged v1 numeric graph profile
with required `source_analysis`, the complete original native source-facts JSON
string. The C++ exporter requires `--preserve-source-analysis`, verifies the
registered module and strict facts schema, and preserves that string exactly.
The default v1 and effects routes still reject source attributes. Unsupported
operation attributes remain rejected; this profile carries one original module
source witness, not inferred source facts from SSA.

The representative source is `source-fixtures/closure.sus`; the original graph
captures 7 and adds argument 2, while a paired mutation captures 11. The graph
is hand-authored and compared against the previously executed original/mutated
v1 graphs. This is not an automatic HIR-to-MLIR source compiler and does not
establish source correspondence for arbitrary supplied graphs.

The authored `bridge --source-graph GRAPH.json ORIGINAL.sus` gate independently
reanalyzes the original source against the real validated bootstrap catalog and
requires exact complete facts before creating the engine or running guest code.
The Rust bridge lowers the verified graph through the existing IR/backend, appends
one `suss.source-analysis` custom section to each independently compiled WasmGC
fragment, checks the encoded bytes with a separate bounded section reader,
performs the existing artifact/ABI validation, then executes with shared recursive
runtime types, a rooted captured closure and forced GC. Typed metadata, lexical
and physical binding identities, original forms, spans and source children are
carried verbatim; no missing fields are synthesized from lowered constructors.
The custom section does not change executable runtime ABI or GC type definitions.

Independent review caught an integrity ordering defect: `compile_ir` seals its
artifact before the source section is appended, and custom sections are covered
by that hash. The bridge now reseals with the existing `annotate_ir`, checks exact
source-section readback after resealing, and verifies artifact identity before
module compilation. An authored regression retains the pre-reseal rejection and
checks resealing, preservation and idempotency. These checks compile and pass in the18-test gate; actual v2 execution passes
before the later main rebase. Current-base rebuild/reverification now passes.

The optional `--execute` gate in `source-fixtures/check-source-export.py` requires
both exact actual result bits and the native source-preservation checkpoints,
and rejects a mismatched original source. Without that option it explicitly
reports native execution as NOT RUN. Source facts are supplementary original
analysis data; numeric operation locations and a complete optimization/debugging
mapping remain separate work. The source section is an isolated experimental
extension, not a shipped artifact-format decision.

Current evidence: pinned C++ one-worker build, complete genuine original/mutated
source export,18 focused Rust tests and actual v2 execution all pass. Both WasmGC
fragments preserve the complete genuine facts after resealing/identity validation,
share the captured closure through rooted cells after GC, and decode exact9/13.
Mismatched original source fails with the exact complete-facts diagnostic before
engine/guest creation; malformed/missing carriers and default-v1 source attributes
fail closed. Explicit null source carriers reject rather than extending v1.
Evidence in ../evidence is labeled before-main: this worktree subsequently moved
to mergedmain22723b4, changing library data. Rebuilding and repeating these gates on that base now passed; final-head
publication gates remain required. Shipped compilation paths remain unchanged.


## Representative executable correspondence

`source-fixtures/source_correspondence.py` derives the expected producer/caller
operations from genuine selected facts for the evaluated two-binding let,
one-capture/one-argument closure and final call. It checks declaration and local
identities, child order, literal binary64 bits, capture order, arithmetic operands
and return/call indexes against the complete executable bodies. Both original
and mutated fixtures pass before-main execution. Corrupted literal, arithmetic
operand, capture index and call argument graphs reject before guest execution.
Independent review exposed Python Boolean/integer and integer/float equality;
recursive exact-type comparison now rejects false capture/operand indexes and a
floating return index as well. Seven graph corruptions reject for each variant.

This is a bounded correspondence check, not a general source-to-MLIR compiler.
It specializes the resolved `suss.core/+` call to numeric addition for the pinned,
oracle-checked fixture environment. Mutable replacement of that global, arbitrary
argument values and general source forms remain outside the profile. Complete
source-facts validation/reanalysis is still required; this helper supplements it.
Module witness preservation alone does not establish arbitrary correspondence or
operation-level source diagnostics. Independent review confirmed the strict-type repair and representative mapping.
The rebuilt current-base executable also passed both actual variants and all
seven corruption negatives. Final-head publication gates remain pending.
