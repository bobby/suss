# Source inference in compiled macro environments

Source analysis retains inferred information separately from physical HIR
storage types. Rooted macro environment graphs expose available `:tag` fields
on source initializer ASTs, locals and declarations. Actual source functions
also expose `:inferred-ret-tag`: an absent field and a present nil value remain
distinct. No source is evaluated to construct these facts.

The executing regression in `compiled_macro_source_tags.rs` failed on the
preceding implementation because its actual 32 source-macro projections lacked
the inferred fields. The repaired regression matches all 32 selected tag
observations from the pinned analyzer corpus in Runtime and Macro caller Stores
after GC, and executes the same 11 runtime projections. A separate effect check
retains an inferred `number` while `(+ "a" 1)` returns `"a1"`; its initializer
runs once. Earlier test fixture failures are preserved in the handoff.

Covered observations include scalar and collection tags, source hints,
do/let/loop/try results, conditional unions, dynamic vars, ordinary versus
function var references, calls, and fixed/multiple function return observations.
The pinned false-literal conditional retains a union; unknown function return
information remains present nil. The comparison selects tag fields only:
native operation/child representations are not fabricated JavaScript ASTs.
This is evidence for those source rules, not complete inference or AST acceptance.

The corpus exposed missing quoted-set lowering when a macro returned a quoted
union tag. Quoted sets now share the existing set constructor/factory path with
literal sets, while recursively lowering their entries as data. The independent
before regression failed with a located unsupported-quote diagnostic; the after
regression executes unresolved symbols, nested list/vector/map data, metadata,
empty sets and 8/9/17-symbol sizes in both Stores. Nested assignment syntax stays
data and performs no assignment. All eight set tests pass, including existing
textual effect order and canonical transport checks.

`hir/source_tags.rs` adapts the pinned analyzer's `get-tag`, inference and return
rules at `c4295f303100bbf5afac449242d30bca1126f1a1`, analyzer.cljc lines
1008–1022, 1529–1659 and 2359–2364. The source SHA-256 is
`297802c627474434f1ef868e31f5f9913c290a4e80c509a40c704dced95bbf47`.
Its upstream copyright and EPL-1.0 notice are retained; the distribution includes
[the EPL license](../../runtime/core-import/epl-v10.html). The native operation
representation and fixture helpers are original repository code. The pinned
JVM compiler and Node are development oracles, not shipped dependencies.

Complete portable AST operations/children, declaration/function/method metadata,
branch refinement and the remaining inference rules still require evidence.
Full `&form` metadata, namespace policy, syntax quote/gensyms, reproducible
versioned Java-free bootstrap, artifact cache invalidation, obsolete evaluator
removal, cancellation and lifecycle acceptance remain open. No issue or M3 gate
is completed by this corpus. Independent PR review and final-head validation
remain required.
