# Rooted compiler analysis graph

`AnalysisGraph` transports actual retained compiler records into canonical values
in a supplied compiled session. It is an original native host implementation and
adds no shipped Java or JavaScript dependency. It is preparation for source-level
`&env`; it does not yet provide the complete ClojureScript environment schema or
change source macro signatures.

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
namespace and declaration schema. Exclusions currently appear as an explicitly
named native vector; persistent set transport is still required for the complete
portable schema. No loaded/initialized status is inferred from declaration records.

Five executing native regressions cover:

* A 96-binding shadow chain, whose final initializer environment and shadow refer
  to the same canonical binding object after forced GC. A compiled function walks
  the entire chain and inspects actual syntax and native arithmetic identity;
  original initializer effects run once in each source Store.
* Named function declaration environments that retain the outer same-named local
  while the function body sees the real self binding, including actual phase.
* Oversized reader metadata rejected before macro-store allocation, caller binding
  publication or source runtime effects.

* An unqualified `/` local key in a hash map with more than eight locals.
* Host-owned declaration metadata at the bridge's legal reader nesting limit,
  independently of analysis record depth. This does not claim equivalent nesting
  support for an enclosing source-reader fixture.

Construction and inspection leave resident fragment/byte and external handle
counts unchanged. Those counters do not establish live GC memory accounting.
The long compiled inspection uses an explicit 100,000,000-fuel stress allowance;
its first default-budget attempt failed. Another attempt failed because the test
expected `Sum` while the actual compiler operation is `Arithmetic::Add`. Both
failures are recorded as failures before the corrected checks passed.

Focused graph tests pass 5/0/0; scope/role/binding tests pass 11/0/0 with new identity
and declaration environment assertions. The required full workspace run completed with
984 passed, 0 failed and 17 ignored across 102 result groups, before the two edge
fixes. Both new edge regressions failed against that version and passed after the
symbol qualification and reader-depth fixes; the final focused graph run passed
all five tests. The initial edge harness had a Rust borrow lifetime error, corrected
before executing it. Full final-head verification remains required before a PR.

Before source macro integration, finish the portable schema, staged
function/declaration metadata and AST inference, then declare/pass actual `&env`
and compare it with the fresh pinned oracle. No M3 acceptance or source-level
`&env` completion is claimed.
