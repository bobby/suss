# Rooted compiler analysis graph

`AnalysisGraph` transports actual retained compiler records into canonical values
in a supplied compiled session. It is an original native host implementation and
adds no shipped Java or JavaScript dependency. Compiled source macro invocation
now passes this rooted graph as implicit `&env`, after `&form` and before user
arguments. It does not yet provide the complete ClojureScript environment schema;
see [source environments](compiled-macro-source-environment.md).

Local and field records retain shared declaration identity tokens across snapshot
clones. Physical ID remapping and receiver/argument adaptations remain separate
cache identities. Memo tables keep the identity owners alive; these pointer keys
are local to one build and are not reproducible artifact cache keys. Function
scopes retain the actual namespace, phase, outer locals and fields at declaration,
before the new self name is installed. Method roles retain whether their source
receiver belongs to a protocol method.

Graph planning uses a bounded work queue. Jobs reserve their result slots before
discovering dependencies; an iterative ordering then constructs canonical rooted
values through `FormBridge`. Each shared record is constructed once. Dependency
cycles fail explicitly. Analysis dependency depth does not consume recursive Rust
stack frames: each initializer can reference the environment of earlier bindings,
even when the reader syntax itself is shallow. The aggregate planning budget is
65,536 nodes and 1,048,576 UTF-16 units. Reader data construction retains its separate
nesting and per-form bounds. Budgets are checked before graph materialization.

Source forms and their actual contexts are preserved alongside explicit native
lowering records. Compiler-only operations stay under `suss` extension keys rather
than being fabricated as JavaScript ASTs. The native representation is not a
substitute for the remaining portable `:op`, inference/tag, method metadata,
namespace and declaration schema. Namespace exclusions now appear under portable
`:excludes` as canonical persistent sets built through the rooted data bridge. No loaded/initialized status is inferred from declaration records.

Nine executing native regressions cover:

* A 96-binding shadow chain, whose final initializer environment and shadow refer
  to the same canonical binding object after forced GC. A compiled function walks
  the entire chain and inspects actual syntax and native arithmetic identity;
  original initializer effects run once in each source Store.
* Named function declaration environments that retain the outer same-named local
  while the function body sees the real self binding, including actual phase.
* Oversized reader metadata rejected before macro-store allocation, caller binding
  publication or source runtime effects.

* Actual namespace `:refer-clojure :exclude` facts exposed as a set with executable
  `set?`, count and membership checks.

* An unqualified `/` local key in a hash map with more than eight locals.
* Host-owned declaration metadata at the bridge's legal reader nesting limit,
  independently of analysis record depth. This does not claim equivalent nesting
  support for an enclosing source-reader fixture.

* Staged definition and function syntax in the explicit `:suss/catalog`, visible in both fixed and variadic method
  bodies while the initializer HIR is still absent, followed by actual execution
  of both signatures in each Store. The prior transport returns nil for this
  syntax and fails the regression.

Definitions retain `initializer_form` before initializer analysis; named function
scopes retain the actual `function_form` before body expansion. The graph exposes
these as explicit native `suss/initializer-form` and `suss/function-form` facts.
They preserve source declarations rather than inferring initialized runtime
status or fabricating portable method metadata. Syntax and analyzed initializer
HIR remain distinct facts.

The environment's `:ns` now uses the immutable namespace at entry to its enclosing
top-level source form, separately from resolution's provisional live catalog.
Nested definitions keep that snapshot, and redefinitions retain the previous
declaration until the next top-level form. Initializer ASTs and function scope
environments share the retained snapshot identity. Declaration record contents,
function facts and inference remain an incomplete portable schema. See
[fresh declaration observations](compiled-macro-declaration-observations.md).

Construction and inspection leave resident fragment/byte and external handle
counts unchanged. Those counters do not establish live GC memory accounting.
The long compiled inspection uses an explicit 100,000,000-fuel stress allowance;
its first default-budget attempt failed. Another attempt failed because the test
expected `Sum` while the actual compiler operation is `Arithmetic::Add`. Both
failures are recorded as failures before the corrected checks passed.

Focused graph tests passed 5/0/0 before staged syntax; the new staged regression
passes in both Stores and fails with the prior graph transport. The final graph/scope/role/binding suites pass 17/0/0, including new identity
and declaration environment assertions. The required full workspace run completed with
984 passed, 0 failed and 17 ignored across 102 result groups, before the two edge
fixes. Both new edge regressions failed against that version and passed after the
symbol qualification and reader-depth fixes; the final focused graph run passed
all five tests. The initial edge harness had a Rust borrow lifetime error, corrected
before executing it. Full final-head verification remains required before a PR.

Before source macro integration, finish the portable schema, staged
function/declaration metadata and AST inference; actual `&env` is now passed
and compare it with the fresh pinned oracle. No M3 acceptance or source-level
`&env` completion is claimed.


Namespace graph data now carries explicit require/use/rename maps for ordinary
and macro imports, preserving source roles and nullable map shape. A fresh
18-field primary corpus matches actual compiled native queries in both phases
after GC; see [namespace data](compiled-macro-namespace-data.md). This remains
partial portable schema preparation; declarations, AST/inference and actual
the complete source environment contract remains unfinished.
