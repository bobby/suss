# Shared script and AOT source preparation

`portable_aot::prepare_source` accepts complete source text, an optional source
path and source roots. It returns prepared Runtime fragments for component
assembly. Native script execution and AOT source preparation share the reader
and staged compilation routine: namespace/dependency preparation, source spans,
compiled top-level macro definitions, expansion and declaration rollback.

The AOT host starts with the shipped Runtime compiler catalog and an isolated
compiled Macro session. Runtime source initializers execute when the resulting
component is instantiated. Source preparation requires compiler facts and never
constructs a Runtime Session. The temporary Macro session can be dropped once
expansion has produced owned compiler artifacts.

Runtime dependencies are compiled in dependency order and included before their
input fragment. Their Wasm retains its own immutable source/phase identity;
staged catalogs supply the shared cells used by component assembly. AOT starts
with suss.core provided and the user namespace selected, then follows explicit
source namespace declarations. Root source paths retain the caller's supplied
identity; dependency lookup records canonical filesystem paths and exact text
hashes. These catalogs come from compilation rather than raw-Wasm reconstruction.

Two executing tests pass. An inline source macro expands code that references a
real Runtime dependency through its alias; the component preserves macro/path
provenance, once-only initialization, subsequent call effects and GC. A second
source prepares successfully despite an unconditional Runtime throw; actual
component instantiation preserves independently decoded first payload17 rather
than later99. The first dependency-path assertion assumed a noncanonical temp
path; checking the loader contract corrected that test setup and strengthened
its exact source-hash assertion. Production lookup behavior stayed unchanged.

Twenty-four existing AOT/native entrypoint, macro, phase and namespace tests pass,
including compile-failure rollback of macro bindings and preservation of completed
dependencies/effects. Both bootstrap images reproduce byte-for-byte without Java,
bootstrap4 and Python118 pass. This CLI-only change leaves the compiler-source
fingerprint and shared ABI layout unchanged. Independent review, required full
baseline and exact final-head CI remain acceptance gates.

The source staging code is original Rust. Existing CLI compile commands remain
legacy. Complete selected-WIT boundaries, frontend migration, evaluator retirement,
published dependency policy and scheduler/cancellation/live heap acceptance remain
open original M3 work. This API and its focused results do not complete M3.
