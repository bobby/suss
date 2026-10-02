# Compiled source macro phase imports in progress

The native compiled host handles explicit `:require-macros` namespace clauses.
Macro source files and their ordinary dependencies execute in the isolated Macro
Store through the same compiler, HIR/IR, ABI and source-backed core as other
fragments. No JVM, Node or host tree walker evaluates macro bodies. Source
snapshots retain selected owned forms, text and locations; dependency discovery
preserves declaration order and distinguishes Runtime/Macro identities even
when both phases use the same file. Provided identities require an actual
matching phase catalog and successful initialization.

The namespace grammar validates macro libspecs with the existing `:as`, `:refer`
and `:rename` rules. Macro catalogs and imports are separate from Runtime cells
and aliases. HIR uses those imports at its actual lexical scope; lexical locals
continue to hide macros. File definitions resolve core aliases/exclusions and
ordinary bindings using the Macro environment, preserving source spans. Ordinary
helper initializers and macro definitions execute in source order. Successful
compiled definitions publish Macro-phase export catalogs from their actual
registered roots. Ordinary aliases/refers within Macro source resolve through
that catalog; this does not infer Runtime macro imports. A module is
marked initialized only after all its forms succeed. Earlier executing Macro
phase effects remain in that isolated Store on failure; Runtime compilation
failure publishes no Runtime input bindings. Caller namespace is restored after
macro loading, including failures. Successful modules initialize once until
session reset; reset discards both phase registries and loaded identities.

Five actual native command regressions cover automatic macro dependencies,
qualified aliases/renamed refers, separate Runtime/Macro dependency cells,
once-only initialization, core definition resolution, ordinary Macro-phase alias/renamed-refer expansion in macro bodies, lexical shadowing,
compile/refer errors and reset. Three discovery tests cover order, same-file
phase identities, actual provided catalogs, cycles and located missing edges.
The existing eleven executing module tests retain their ordinary preparation
contract. Hosts without compiled macro support still return located diagnostics;
an accepted clause alone never claims the macro function is loaded.

This remains partial M3 integration. Complete source reload/ordinary require policy, artifact cache invalidation,
private/export policy, inferred imports, mixed Runtime
macro-definition inputs, complete AOT command integration, full &form/&env data,
syntaxquote/unquote/splicing/gensyms, versioned bounded Java-free bootstrap,
complete cache keys and legacy evaluator removal remain required. No complete
bootstrap verifier exists yet. Cooperative cancellation/pending I/O cleanup and
actual live GC accounting remain required by #15. No milestone is complete.

The implementation is original Rust. Namespace lookup behavior was checked
against pinned analyzer.cljc4177–4229 at
c4295f303100bbf5afac449242d30bca1126f1a1 (EPL1.0); no upstream text was copied.
The structural defmacro adaptation retains the provenance recorded in
[compiled source macros](compiled-source-macros.md).

Explicit macro libspec reload/reload-all now executes phase source refresh;
[reload evidence](compiled-macro-reload.md) preserves its acceptance limits.
