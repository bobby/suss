# Pinned declaration environment observations

These are development-only primary observations for M3 issue #14. They do not
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
does not run a native comparison. The existing native graph's backend declaration
facts must not be interpreted as the certified portable schema.

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

The corpus preserves 13 ordered observations and 14 executed results. Its direct
parameter-returning functions have no observed return tag. Do not substitute
`any` or a physical closure type for absent inference facts.

Still unobserved here: nested definitions within a single top-level form,
declaration macros and their argument-list metadata, duplicate arity methods,
named self-binding metadata, complete return/union inference, namespace reload
policy and analyzer options. Observe those boundaries before extending portable
behavior. Next implement separate immutable portable namespace snapshots and
live resolution facts, preserve staged source function facts before compiler
wrappers, and add native executing comparisons using the reviewed corpus.
