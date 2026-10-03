# Compiled function declaration facts

Direct source function declarations now project six selected portable
declaration fields to compiled macros: `:fn-var`, `:variadic?`,
`:max-fixed-arity`, `:method-params`, `:arglists` and `:arglists-meta`.
Their default values come from immutable analyzed source callable facts and verified
reader metadata. Runtime closure storage does not supply these facts.

Non-nil `:top-fn` merge data replaces the computed arity and argument-list group,
retaining selected field presence, collection kinds and values; they may also
replace `:fn-var`. A two-element entry vector can replace `:fn-var`; an empty map or sequence
leaves the five computed fields absent,
while nil uses normal callable facts. Truthy `:declared` metadata retains raw
symbol metadata for these selected fields instead of publishing completed
callable facts. This projection does not complete catalog timing or the entire
portable metadata schema.

Every original signature remains in `:method-params`, including duplicate fixed
arities; each method is a vector of parameter symbols, with the rest parameter
retained and `&` omitted. The containing value is a list. Emitted dispatch still
executes the last duplicate body. A function alias has absent function fields.
Old namespace snapshots retain old function declarations during redefinition;
the provisional catalog entry has no completed function facts. A completed scalar
replacement removes those fields.

Direct functions without argument-list metadata have present nil `:arglists`
and a present empty list for `:arglists-meta`. Explicit argument lists preserve
the reader data, including its quote. The metadata list describes elements of
that value; the pinned quoted fixture yields nil for `quote` and source metadata
for the following list. A map-valued argument-list fixture retains its map and
has nil metadata for its map entry. Source provenance enriches only verified
original reader data. It never evaluates the initializer again.

The development-only pinned analyzer helper is original test code. Its corpus
now retains 29 snapshot/catalog observations, two self/local observations and
37 actual Node results from ClojureScript
`c4295f303100bbf5afac449242d30bca1126f1a1`. The original 21 observations and
26 executed result prefix remain unchanged. No upstream analyzer implementation
or additional dependency is copied into shipped code.

Native tests execute the shared selected projections in both caller phases,
force GC, check duplicate/alias/variadic calls and redefinition timing, and compare
quoted argument lists plus exact reader positions. The loaded native module's
canonical file path is asserted independently before the primary fixture path
normalization. The full core namespace remains subject to the existing graph
and occurrence-work bounds.

Commands, failures and terminal results are recorded in the handoff. Independent
PR review, full workspace baseline and successful final-head CI are required.
This is partial progress on issue #14. Complete declaration field presence,
argument metadata outside the observed cases, self-local method binding records,
return/union inference, source/macro cache invalidation, evaluator retirement and
M3 lifecycle acceptance remain open.
