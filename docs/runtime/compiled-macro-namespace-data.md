# Compiler namespace data in the native analysis graph

Actual retained namespace facts now expose `:requires`, `:uses`, `:renames`,
`:require-macros`, `:use-macros` and `:rename-macros` through canonical compiled
map/symbol data. Ordinary and macro phase catalogs remain separate. Required
libraries retain explicit library-to-library entries even without an alias;
optional aliases add further entries. These are compiler facts, not certificates
of initialized Runtime cells.

Source referral roles remain explicit in immutable snapshots. An unchanged-name
rename such as `:rename {one one}` remains a rename. Repeated specs can place the
same original binding in both use and rename maps. Public programmatic refer
configuration retains its existing spelling-based role, while source directives
supply the actual explicit role. Resolution identity and ambiguity checks remain
unchanged. Core rename directives also record the rename role.

Unused ordinary requires/uses and macro requires remain nil. Rename, macro-use
and macro-rename fields remain maps even when empty. The development projection
records field presence and nullable shape, so it does not turn nil into an empty
map. Entry sorting normalizes only map enumeration; symbols and targets retain
exact values.

The contract source is the pinned ClojureScript analyzer at
`c4295f303100bbf5afac449242d30bca1126f1a1`, particularly `parse-require-spec` and
`merge-ns-info` in `clojurescript/src/main/clojure/cljs/analyzer.cljc`. The native
host transport and development projection are original Suss code; this change
ports no core algorithm and introduces no shipped Java/Node dependency or runtime
ABI layout change.

`tests/oracle/namespace-environment-observations.json` retains exact observations
for populated, ordinary-empty and macro-empty namespaces. Fresh compilation uses
`:force true` and `:cache-analysis false`, verifies the checkout pin, executes Node,
and rejects duplicate/missing/changed projections and results. Native queries
read the same expected corpus from actual source namespaces and compiled macro
imports, in both phases after forced GC. They preserve resident code and external
handle counts during graph construction/inspection. These counters do not prove
live GC memory accounting.

Run `scripts/test-namespace-environment-oracle.sh` with the usual bounded Cargo
configuration. The handoff records actual terminal results and failed attempts.
Initial fixtures incorrectly put multiple namespace declarations in one input
and omitted provided source modules; those harness failures are separate from
the executed missing-map regression. The first projection normalized nullable
shape; its stronger replacement revealed a reviewed expected-data mismatch and
an actual native empty-map-versus-nil failure before repair.

This evidence covers the six explicit import maps for the tested source namespace
forms. Default REPL namespace shape, repeated namespace declaration/reload policy,
implicit macro autoloading, complete declaration/AST/inference data and actual
source macro `&env` invocation are not certified by this projection. Original M3
acceptance remains open.
