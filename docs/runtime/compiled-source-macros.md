# Compiled source macro expansion in progress

Native CompiledMacros compiles defmacro definitions as functions in
its isolated Macro-phase Store. Session::eval_with_macros uses the compiler's
ExpansionHost interface. HIR analysis calls the host at the actual lexical scope;
local/field bindings shadow unqualified macro names, quoted data bypasses expansion,
and compiler special forms retain priority. Expansion returns forms to the same
HIR/verified IR/Wasm pipeline. Dependency artifacts use the same host before any
runtime publication. Macro bodies execute in Wasm, never in a native tree walker.

Arguments and &form are actual GC-owned data. The host captures definition roots,
so failed replacement preserves the prior macro. A successful replacement affects
future expansion; previously compiled runtime artifacts retain their original
expansion. Expansion recursion uses the existing bounded analysis-depth diagnostic.
Runtime compile failures publish no bindings/modules; compile-time body effects
remain isolated in the Macro Store. Macro exceptions and invalid data become
located compile errors at the call site. Explicit registration is required; this
is not yet automatic namespace macro loading or the native prompt default.

The structural definition adaptation follows pinned core.cljc defmacro3440–3481,
SHA25680f2964a97e3cf5bb7adde5eb9fabcb5fe014040ab8c29217c19e8b13679f171,
commitc4295f303100bbf5afac449242d30bca1126f1a1 (EPL1.0). Implementation is original
Rust, not copied upstream source. The review stays in-progress. Single-vector and multi-signature declarations, docstrings and ordinary attribute
maps are structurally retained through compiler definition forms. Attribute maps
are preserved as source metadata, not yet observable persistent runtime maps.
Privacy/const/macro/export attributes retain explicit compiler errors. Namespace
registration retains independent function roots. Macro bodies use the same
lexical expansion host against an immutable compiler snapshot, then install in
the original Macro Store; successful prior expansions in compiled bodies stay
stable after macro replacement. Runtime macro marker/Var metadata, &env, full
&form metadata and automatic namespace aliases/requires remain unfinished. Bare &env
is an explicit unresolved compile error; no placeholder nil environment is passed.

Syntaxquote/unquote/splicing, deterministic gensyms, persistent metadata/collection/
complete sequence transport, macro phase dependency loading, Java-free reproducible
versioned bootstrap, complete cache keys/invalidation and evaluator removal remain
required by #14. This host exposes real analyzed local binding context for future
&env integration but does not claim its runtime map representation exists. #15
cancellation/pending interactive I/O/live GC accounting remains unfinished. M3
stays open, and the legacy evaluator remains until bootstrap acceptance passes.
