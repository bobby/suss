# Native compiled REPL macro integration in progress

The native `suss repl` owns one persistent Runtime Store and one isolated Macro
Store. A standalone top-level defmacro input compiles in the Macro Store and
prints its actual function result through the existing native formatter. This is
not a fabricated runtime Var or macro-marker object. Later inputs expand through
that Store, then compile/install/execute only their new Runtime fragments. No
source replay or native macro-body evaluation occurs. Old compiled functions keep
their original expansions after macro redefinition.

The input compiler receives owned parsed/selected forms, preserving UTF16, spans
and metadata. Core-qualified/aliased definitions, core exclusions and ordered reader conditionals use the
same path. A resolved runtime binding named defmacro retains normal call semantics.
Namespace registration follows the current Runtime namespace while preserving
Macro-phase cells and owned roots. Load/reload/reload-all also prepare dependency
artifacts with the same expansion host before publication. Failed compilation
publishes no Runtime input bindings; preceding effects of executing language
errors retain existing session semantics.

Reset provisions both complete replacements before discarding either old Store.
Failure while provisioning the second replacement keeps both old phases, roots,
macros and Runtime bindings. Successful reset discards both registries/Stores and
invalidates old handles. Fuel exhaustion remains a trap; this does not implement
cooperative cancellation, pending I/O cleanup or live heap accounting.

This is native prompt integration of the current supported macro subset. Mixed
inputs containing source macro definitions and runtime forms, nested defmacro,
complete macro namespace reload/privacy/cache policy, complete AOT/frontend sharing,
&env, full runtime &form metadata, syntaxquote/splicing/gensyms, persistent
collection/lazy/metadata data, versioned Java-free bootstrap, complete cache keys/
invalidation and legacy evaluator removal remain required M3 work. Unsupported
forms keep located diagnostics. `scripts/verify-bootstrap.sh` is still absent.

Five `compiled_repl_macros` regressions execute the actual native command for
once-only effects, old expansions, variadic data output, namespace registration,
failed definitions/expansion, error recovery, reset, core aliases/conditionals and
runtime-name shadowing. Source file load/reload executes changed macro expansions;
a sixth host second-phase reset-failure regression preserves both original phases.
The implementation is original Rust integrating the partial pinned defmacro
adaptation documented in compiled-source-macros.md. No new upstream source is
copied or selected, and no ABI/dependency/shipped JVM/Node requirement is added.

Explicit `:require-macros` source imports now execute through the isolated Store;
see [phase imports](compiled-macro-imports.md) for evidence and remaining scope.
