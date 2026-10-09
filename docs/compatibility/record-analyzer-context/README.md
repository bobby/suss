# Record and reify compiler dependency retention

The ledger retains 46 complete declarations and source stanzas from the pinned
ClojureScript checkout, including the complete extension helper graph, analyzer
resolution/type parsing, compiler name munging, protocol-mask tables, gensym and
`nil-iter`. The 12 original record macro declarations remain separately retained
under `record-macro-foundations`. Both directories retain upstream EPL notices.

Run `python3 -B scripts/check_record_analyzer_provenance.py` to check the closed,
ordered dependency inventory, exact source ranges, reader contexts, complete
source bytes, hashes and notice against the actual clean pin. This verifies
retention only. These forms have not been imported or executed by this change.

`portable/compiler_facts.rs` exposes actual caller resolution, lexical bindings
(including unused locals), protocol signatures and reader-metadata elision.
`portable/compiler_names.rs` adapts the complete scalar UTF-16 name-munging path
and source tables. The macro analysis graph uses the same live resolution catalog.
The authored Rust regressions have not been compiled or executed.

Cross-namespace catalogs preserve actual phase-specific scopes and declaration
revisions without switching namespaces. A declared namespace without a source
scope remains an explicit missing scope. Namespace-segment matching follows the
whole pinned `get-first-ns-segment`/`find-ns-starts-with` helpers, not substring
matching. An explicit `:suss/compiler-catalog true` macro declaration now requests an
additional `&env` compiler catalog in the compiled Macro Store. It contains the
actual caller phase/current namespace, all actual phase-specific namespace
revisions (nil for a declared namespace without a source scope), and actual
protocol method arities. Existing source graph construction/limits and declaration
identity sharing remain in use. Ordinary macros retain their existing graph.
False/nil or absent policy does not request the catalog; malformed policy fails.
Policy rolls back with callable bindings when script staging fails during preparation.
Namespace reload has a different boundary: definitions published before a later
initializer failure remain published; a successful reload can replace them.

Four authored consuming source-macro regressions check independently decoded
namespace names/docs/overloads and retained results after GC, opt-in/malformed
policy, failed-script staging rollback, and partial namespace publication followed
by successful reload recovery. They are UNCOMPILED/UNEXECUTED. These
transport keys are a Suss compiler interface, not an assertion of upstream
analyzer-state schema equality. Canonical namespaces remain `suss.core`; no
fabricated `cljs.core` revision or extern/module/warning state is inserted.
Analyzer extern/module/warning mutations are still missing.

These interfaces do not implement all analyzer `resolve-var` branches: extern
state, module resolution, warning callbacks and fallback definitions still need
their source-faithful adapter. Map-valued compiler munging requires genuine
shadow/function-scope/lexical-rename facts. Executing the complete helper graph
also requires compiler-state updates, anonymous captured class publication,
protocol-mask and annotation integration, and source macro staging. `exists?`,
`reify`, the complete `deftype` helper graph and the record factories are not
implemented by retaining their dependencies. `nil-iter` remains unimported until
its genuine reify path can execute. Ordinary deftype capture restrictions and
transactional publication must remain intact when that path is implemented.

The 336 existing recipe selections and the seven reify/fourteen iterator cases
are unchanged. No bootstrap, shared runtime ABI or native acceptance result is
updated here; full record/reify and original issue acceptance remain pending.
