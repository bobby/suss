# Native compiled phase sessions

A native `Session` has a fixed Runtime or Macro phase. Existing constructors keep
Runtime behavior. `with_options_in` and `with_engine_in` choose a phase explicitly;
`new_macro` creates a separate Macro Store and compiles the same retained core
profile used by `new_repl` through the same portable compiler pipeline.

The phase governs initial intrinsic cells, arithmetic cells, core provided-module
identity, input and source dependency preparation, namespace scope, reload and
reset. It is not an annotation on runtime execution: emitted macro imports use
`suss.bindings.macro`, with their own host cells and Wasmtime Store. Runtime and
Macro sessions cannot accept each other's rooted values. A shared Engine may
cache code, but no bindings, instances, mutable language state or roots are shared.
There is no source replay or tree-walking body execution.

Reset creates and provisions a replacement in the original phase before replacing
the old Store. A failed replacement leaves the previous state and roots usable.
A successful reset rejects old roots. Namespace source loading/reload has the same
compile-atomic and initializer-error policies in both phases; loaded identities
and live cells remain phase-qualified. An ordinary require in a Macro session
loads source code into that phase. Cross-phase `:require-macros` integration remains
an explicit unsupported diagnostic.

`compiled_phase_session` executes real native artifacts for separate core/global
mutation and root rejection, retained symbols/lists, compiled transformer function
invocation, dependency reload/failure/retry and phase-preserving reset. These tests
establish the native execution prerequisite. They do not establish source macro
expansion: defmacro recognition, syntax quote/unquote/splicing, deterministic
gensyms, &form/&env, full collection/metadata data, the reproducible versioned
Java-free bootstrap and cache invalidation remain required by #14. The legacy
MacroEvaluator is still present until that acceptance gate passes. Lifecycle
cancellation and actual live-GC accounting remain #15 work.

Implementation is original Rust. Existing retained core source keeps its upstream
provenance and EPL notices; no new source port, runtime ABI revision, dependency,
shipped Java or JavaScript requirement is introduced.
