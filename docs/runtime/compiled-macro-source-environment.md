# Implicit compiler environments in source macros

`CompiledMacros` passes `&form` and `&env` before user arguments in every fixed,
multiple and variadic source macro signature. Macro bodies execute in the isolated
compiled Macro Store. The environment is canonical rooted data constructed from
the actual caller `ExpansionContext` through `AnalysisGraph`. It carries immutable
lexical records, function scopes, context and namespace snapshots alongside
explicit backend facts. Building it does not execute caller initializers.

Namespace `:ns` retains the enclosing top-level snapshot. The explicit backend
`:suss/catalog` carries live compiler resolution data, including staged definitions.
Definition records now retain `:suss/definition-form` from the syntax actually
analyzed and `:suss/analysis-completed`. Completion is independent of initializer
presence and runtime execution: a definition without an initializer can have
completed analysis, while an initializer under analysis has not completed.
Expanded syntax remains expansion/call-site provenance, not proof that generated
syntax appears in the original source text.

Three executing regressions failed before the implicit argument was added with
an unresolved `&env` diagnostic. They pass after it is added, in Runtime and Macro
caller sessions with GC before result inspection. They check lexical initializer
and shadow records without replaying effects, function scope, zero/fixed/variadic
signatures, and snapshot versus staged/redefined documentation. A fourth executing
graph regression retains complete definition syntax, explicit name metadata and
documentation through staging/redefinition, including completed analysis without
an initializer. The affected existing definition/import/reload/REPL macro suites
pass 21 tests with zero failures or ignores.

`sh scripts/test-source-environment-oracle.sh` forces the pinned ClojureScript
compiler, observes actual source macro invocation and executes its Node artifact.
A second fresh run matches three ordered projections and four executed results;
independently decoded native macro results match the shared projected corpus in
both caller phases. This projection deliberately excludes symbol metadata and
spans; it does not certify those fields. The helpers and fixtures are original
repository code. The development compiler retains its pinned source and EPL
notices; Java and Node are not shipped dependencies.

Complete portable declaration/function/method metadata, source AST operations
and inferred tags remain unfinished. Native lowering facts stay under `suss`
extension keys; they are not fabricated JavaScript ASTs. Full `&form` metadata,
namespace default/reload/privacy policy, syntax quote/gensyms, reproducible
versioned Java-free bootstrap, full cache invalidation, legacy evaluator removal
and scheduler/lifecycle acceptance remain required. No M3 issue or milestone is
closed by this bounded source-invocation evidence. This continuation still needs
independent PR review, the required full workspace baseline and final-head CI.

The internal compiled macro calling convention gains one implicit argument;
macro definitions from the previous convention must be recompiled from source.
The shared runtime ABI stays at version 2: its universal invocation already takes
an argument array. This does not establish persistent macro artifact cache
compatibility or invalidation acceptance.
