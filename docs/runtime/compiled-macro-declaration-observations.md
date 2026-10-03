# Pinned declaration environment observations

These are primary observations and a native snapshot timing repair for M3 issue #14. They do not
establish native portable declaration transport, source AST inference, implicit
`&env`, Java-free bootstrap or milestone completion.

The original helper in
`tests/oracle/src/suss_oracle/declaration_macros.clj` observes actual macro
`&env` and separately calls the pinned analyzer's public `get-namespace` helper.
No upstream analyzer implementation is copied. The fixture is compiled with
fresh analysis from ClojureScript commit
`c4295f303100bbf5afac449242d30bca1126f1a1` and its emitted Node artifact executes.
Java and Node are development dependencies only.

Run `scripts/test-declaration-environment-oracle.sh`. The script checks the actual
upstream checkout, clears the call log, force-compiles with analysis caching
disabled, executes Node and compares exact observations with
`tests/oracle/declaration-environment-observations.json`. This primary-only script
does not run a native comparison. The native graph test below separately executes
the observed snapshot timing. Backend declaration facts must not be interpreted
as the complete portable schema.

Each observation contains a label, namespace identity, selected declarations
from `&env`'s namespace snapshot and the same selected declarations from the live
analyzer catalog. Declaration and field presence are separate booleans. Symbols,
keywords, vectors, sequences, sets and maps have explicit data kind tags; an
empty sequence is distinct from nil. Source file fields alone lose the verified
absolute checkout prefix. Other strings, line/column values, reader end positions
and metadata remain exact.

The fresh fixture establishes the following timing for these cases:

* Before definition, both collections lack the selected new names.
* During the scalar initializer and function bodies, the namespace snapshot
  lacks the new declaration. The live catalog already has its provisional name,
  source positions and symbol metadata.
* After initializer analysis, both collections have final declaration metadata.
  Explicit definition documentation replaces the symbol documentation in the
  declaration's `:doc`, while `:meta` retains the original symbol documentation.
* Direct fixed and multi-arity functions acquire `:fn-var`, `:variadic?`,
  `:max-fixed-arity`, `:method-params`, `:arglists` and `:arglists-meta` afterward.
  Direct `fn` definitions in this fixture have present nil `:arglists` and a
  present empty sequence for `:arglists-meta`. Rest parameter vectors omit `&`
  and retain the rest parameter symbol.
* Assigning an existing function to a new variable does not give that declaration
  `:fn-var`. Physical callable storage is insufficient evidence for this field.
* The dynamic numeric declaration has portable tag `any`. An explicitly hinted
  string declaration has tag `string`, even though the macro initializer returns
  the number 42. Portable analysis tags cannot be inferred from backend storage
  types or used as unchecked runtime casts.
* During scalar redefinition, the snapshot retains the old declaration, including
  its documentation and private flag. The provisional catalog entry is already
  replaced. Both have the new declaration after analysis completes.
* Selected declaration records do not have a `:ns` field. Resolution information
  may have a namespace; that is a different schema.
* Nested definitions update the live catalog while the enclosing top-level
  definition keeps its original namespace snapshot. The next top-level form sees
  the completed nested declarations.
* `declare` publishes `:declared true`; replacing that declaration with a direct
  function publishes its analyzed function facts afterward.
* Duplicate fixed arities emit a primary warning and execute the last body, but
  declaration `:method-params` retains both source parameter lists. Deduplicated
  emitted methods are insufficient evidence for declaration method metadata.
* Named self locals have `:fn-var`, `:variadic?`, `:max-fixed-arity` and
  `:method-params` within their bodies. These method parameters are analyzed
  binding records, rather than the symbol vectors in global declarations. The
  fixture preserves exact binding data, positions and present nil parameter tags.

The corpus preserves its original 13 observations and extends it to 29 ordered
snapshot/catalog observations, two self/local observations and 37 executed results. Its direct
parameter-returning functions have no observed return tag. Do not substitute
`any` or a physical closure type for absent inference facts.

The native analyzer now retains a separate immutable namespace snapshot at entry
to each top-level source form. Resolution still uses the live environment. Source
analysis records and named function declaration scopes carry both facts. Queued
graph transport exposes the snapshot under `:ns` and the live catalog under the
explicit backend extension `:suss/catalog`. Initializer syntax and pending HIR
facts belong to that backend catalog; they do not imply initialized runtime vars.
The declaration record contents still need complete portable schema work.

`native_analysis_graph_separates_top_level_namespace_snapshot_from_live_catalog`
executes initial definition, redefinition and nested definition queries in both
Stores after GC, then executes the resulting runtime values. It failed against
the old transport and passed after the repair. Existing staged syntax queries now
read `:suss/catalog`. Additional executing queries check shared snapshot identity
through initializer AST and function declaration environments.

The added explicit argument-list fixture preserves the actual quoted reader
sequence in `:arglists`. `:arglists-meta` maps metadata over that quoted value's
elements: nil for its `quote` symbol, then the reader metadata of its argument
sequence, including verified file and exact line/column/end positions. It does
not map metadata over the individual arity vectors. A separate map-valued
argument-list fixture is accepted by the pinned analyzer: it preserves the map
data and gives its single sequence entry nil metadata. The original 21 cases, two
local observations and 26 executed result prefix remain unchanged.

Still unobserved here: complete return/union inference, namespace reload policy
and analyzer options. Observe those boundaries
before extending portable behavior. Next preserve staged source function facts
before compiler wrappers and build portable declaration metadata/inference with
native executing comparisons using the reviewed corpus. Actual source implicit
`&env` invocation remains unfinished.
