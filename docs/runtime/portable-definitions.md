# Source definitions and namespace preparation

The portable compiler now lowers source `def` and a bounded `defonce` bootstrap
through the same HIR/IR/shared-runtime pipeline as ordinary expressions. A leading
`ns` directive configures a private environment snapshot. This is compiler and
runtime groundwork, not a production session or recursive source loader. CLI,
AOT, source-replaying REPL and macro clients still require migration.

## Compilation and publication

`prepare_fragment(source, &environment, phase)` returns validated Wasm, the staged
Environment, current-phase cell identities and the retained namespace directive.
An error cannot mutate the supplied Environment. All source callers use the same
reader, namespace preparation, HIR, IR verifier and backend. `compile_in` returns
only Wasm; clients that need new declarations should use `prepare_fragment`.

A host must reuse existing cells by resolved Global identity, create missing
unbound cells, import the shared runtime and verify manifests/layouts before
execution. Compilation must finish before publishing the staged environment or
allocating session bindings. The test harness follows that order in one Store;
it is not a shipped session API, module cache or lifecycle policy. Current-phase
cells are separated from the other phase's catalog; this does not establish an
isolated compiled macro session.

Definitions declare their identity before analyzing the initializer, allowing
self references. Definition names, source spans, ordered metadata and optional
UTF-16 docstrings remain in HIR. A definition initializer evaluates once before
GlobalWrite publishes its value. The write returns that same value through the
universal Value boundary. A failed initializer preserves the old cell value;
arbitrary preceding effects, including other completed definitions, are not
rolled back. An initializerless declaration preserves an existing binding and
leaves a new cell unbound; its fragment result is nil. Source-declaration use as
an expression is not separately certified: the pinned compiler produced invalid
JavaScript when a no-initializer def was supplied directly as a println argument.

`defonce` checks the cell's bound flag before entering its initializer block.
Bound nil and false count as initialized. A skipped initializer performs no
effects and returns nil. The initializer is still analyzed, and declarations
inside a skipped body remain unbound. Bootstrap macro resolution retains canonical
core/refer identities, respects exclusions and is hidden by lexical locals;
true `def` syntax is unshadowable. Names may be unqualified or qualified by the
current namespace; definitions into another namespace are rejected.

Storage operations are explicit IR instructions: GlobalBound has a Bool result,
GlobalWrite consumes an already normalized dominating value and has a dynamic
Value result. Verification rejects malformed facts/undefined operands before
emission. `binding-bound` returns the exact language Boolean sentinels. The ABI
recursive layouts remain unchanged; an older runtime lacking the new intrinsic
fails linking before eval. Nested closure bodies collect their own cell imports,
and GC locals retain the initializer value across publication.

## Namespace boundary

A leading top-level `ns` accepts a symbol name, optional docstring/attribute map,
`:require` symbol/libspecs with `:as`, `:refer` and `:rename`, and
`:refer-clojure` exclusions/renames. Required namespaces and referred names must
already exist in the supplied declaration catalog. Duplicate clauses/options,
ambiguous aliases/refers, malformed names/options and missing declarations fail
with source byte locations. `cljs.core` selects the canonical `suss.core` identity.
A fresh ns declaration replaces that phase's import/exclusion scope while retaining
its existing cells; API `enter_namespace` preserves scopes for future REPL re-entry.
The original ns directive, including metadata, is retained separately from runtime
values. Reader metadata is not claimed as runtime metadata.

This does not locate/load dependency graphs, verify a file's declared namespace,
handle cycles, privacy, reload/cache/initialization policy or permit multiple ns
changes inside one fragment. Source `:require-macros` is a located unsupported
feature until isolated compiled macro imports exist. `const`, `dynamic`, `private`,
`macro` and `export` definition attributes also fail explicitly rather than being
silently ignored. General runtime metadata/nominal types/protocols/exceptions,
extended signatures, dynamic numeric coercions and portable core values remain
unfinished.

## Executing evidence and provenance

Ten definition tests execute actual fragments in a long-lived test Store. They
cover live rebinding versus old captures, definitions inside functions, bound
nil/false and skipped effects, compiler snapshots, failed initializer publication
with preserved effects, namespace scope replacement, canonical aliases/renames,
separate phase identities and malformed public IR. Test-only Wasm catchers catch
the exact shared language tag and independently decode descriptor/message/nil
payloads after GC; traps cannot pass as language exceptions.

The original portable corpus now contains 42 source cases. Fresh pinned
ClojureScript/Node observations match source definitions, declarations, lexical
initializers, live function bodies and defonce effects/nil/false, alongside the
previous scalar/function cases. A separate fresh fixture confirms def returns its
value, declaration statements preserve bindings and local definitions execute.
The initially invalid declaration-as-argument oracle fixture failed and was
corrected to statement context, not skipped or reported successful.

Implementation/tests are original. Semantics were checked against
`cljs/core.cljc` (`defonce`) and `cljs/analyzer.cljc` (`def`, `ns`) at pinned
`c4295f303100bbf5afac449242d30bca1126f1a1`, licensed EPL-1.0. No upstream
implementation was copied; a full upstream port still needs extraction/patch
hashes and retained notices. JVM/Node remain development-only. All 1,065 inventory
declarations remain unassessed and legacy differential results stay 9 passes /
7 exact failures / 0 skips. M2/M3 acceptance is not complete.

Next integrate recursive source namespace/module loading and persistent compiled
clients, including deterministic dependency errors and initializer policy. Then
extend signatures/core coercions/collections and the remaining IR forms, compiled
macros and target adapters. Retire the legacy backend only after replacement
acceptance. Commands and actual review/CI evidence belong in the handoff and PR.
