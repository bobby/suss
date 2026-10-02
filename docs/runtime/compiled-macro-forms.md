# Compiled macro form and data transport

`prepare_fragment_forms`, `prepare_input_forms` and native `Session::eval_forms`
accept owned reader forms with their existing source spans and reader metadata.
Source preparation delegates to the same path. Conditional selection happens
before namespace/dependency preparation, followed by HIR, verified evaluation-order
IR, Wasm validation, staged linking and execution. Forms are not printed and
reread. This preserves UTF16 code units, binary64 payloads and caller locations;
compile errors still publish no bindings or modules. Dependency graph snapshots
continue to use exact source text and the existing phase-specific namespace rules.

Native `FormBridge` quotes a form through that pipeline into real GC-owned values
and reads a compiled transformer's result into forms for the same pipeline. Macro
bodies run in Wasm, rather than in a native tree walker. Captured canonical class
roots establish nominal identity independently of Wasm structural matching; they
survive redefinitions and collection. A bridge or value from another Store or a
previous reset is rejected. Construct one bridge for the compile-time session,
then discard/recreate it on reset. Its five class roots are ordinary external
owned roots; capture compiles five cell reads once, not macro body replay.

The current transported types are nil, booleans, boxed binary64, exact UTF16
strings, source-backed Symbol/Keyword and List/EmptyList/Cons sequences. Actual
class descriptor identities, object/field layouts, identifier encoding, metadata
and list counts are checked. Flat sequence tails are traversed iteratively; nested
forms use a depth limit of64. A4096-visit budget also rejects cycles, and a shared
1,048,576-code-unit budget bounds the total copied string/identifier storage.
Unknown objects, null/invalid storage, malformed counts and unsupported metadata
produce located compile diagnostics rather than successful placeholder forms.
New result forms inherit the explicitly supplied macro call-site span; no source
bytes or original result locations are invented.

This is transport and compilation integration, not completed source macro
expansion. Persistent vectors/maps/sets, IndexedSeq and lazy sequence results,
runtime metadata maps and full &form/&env representation remain to be integrated.
Source defmacro/require-macros, syntax quote/unquote/splicing, deterministic gensyms,
phase dependency integration, versioned reproducible Java-free bootstrap, full
cache invalidation and removal of the legacy MacroEvaluator remain #14 work.
Cancellation/pending I/O/live GC accounting remain #15 work. No M3 completion.

`compiled_macro_forms` runs actual native artifacts for lossless scalar forms,
located/metadata/compile-atomic failures, phase dependencies and reader conditional
selection, compiled transformer result execution in the Runtime Store, captured
nominal identities, malformed data, reset/foreign roots, cycles, wide/deep data and
total UTF16 allocation bounds. Existing phase/session/quote suites remain enabled.
Implementation is original Rust using existing retained source types and the ABI;
no new source selection, ABI, dependency or shipped JVM/Node requirement is added.
