# Compiler function scope facts

This is partial issue #14 work toward genuine compiled macro `&env`. Source macros
receive rooted source graph records; the full portable schema remains incomplete.
The expansion host borrows actual
named function scopes, preserving declarations, source origins, phase namespace,
parent scopes and lexical/field shadows.

An explicit self name retains its real compiler local binding. A definition name
hint names the scope without adding a local or inventing a binding ID. Explicit
names override hints. Anonymous nested functions inherit the enclosing named
scope; sibling or subsequent forms do not inherit a completed function scope.
Name hints follow macro expansion, but are not propagated into arbitrary operands
or bodies. Parent records share immutable references without cycles.

The development-only oracle force-compiles pinned ClojureScript with analysis
caching disabled. Eleven actual ordered compiler observations cover definition
hints, named/anonymous/nested functions, inherited scopes, shadowed locals,
multiple arities, rest parameters and restoration. Ten executed name vectors
were captured from Node. The strict comparator checks exact names and lexical
name sets; it is not a general runtime value decoder. The native fixture now
compares those observations and decodes actual canonical vectors after GC in both
Stores, while inspecting binding IDs and structural parent/shadow records.
Independent review added an executing regression for definition hints through
macro expansion, explicit generated self names, sibling restoration and dropping
hints inside arbitrary let bodies. Generated self declarations cannot fabricate
a source token position; the owned-forms compiler API keeps absent origins absent.
The two scope tests and seven binding/context/field regressions all passed.

```sh
CARGO_BUILD_JOBS=2 scripts/test-function-scope-oracle.sh
cargo test -p suss-cli --locked --test compiled_macro_function_scopes \
  --test compiled_macro_binding_records --test compiled_macro_analysis_context \
  --test compiled_macro_field_records -- --test-threads=2
```

Function ASTs now retain the genuine name scope before function analysis restores
its enclosing scope. Anonymous functions expose present `:name nil`, omit
`:local`, and declare `[:methods]` children. Named functions and definition hints
share one binding AST between `:name` and `:local`, with `[:local :methods]`
children. This record is also shared with the method-entry function scope.
An anonymous nested function retains its enclosing environment without acquiring
its parent's name AST.

The binding AST carries actual `:op :binding`, source `:form`/`:name`, `:local :fn`
and declaration environment. Its `:info` records `:fn-self-name`, namespace,
parent scopes and present nullable shadow. `:ret-tag` preserves raw non-nil name
metadata, including false; nil does not create this key. Definition hints keep
actual declarations and scopes without inventing lexical binding IDs.

Seven fresh pinned analyzer observations agree exactly with raw Node output.
They inspect21 selected fields for anonymous/named/shadowed functions, number,
false and nil return tags, and multiple methods; all executions return42.
The native regression consumes that same corpus in both caller phases after GC.
The isolated parent executes42 but fails on absent anonymous `:name` presence.
Compiler regressions additionally check shared actual scope identity, definition
hints and an anonymous nested function's inherited environment.

```sh
scripts/test-function-name-ast-oracle.sh
cargo test -p suss-compile --locked --test portable_control_source_analysis \
  -- --test-threads=2
cargo test -p suss-cli --locked --test compiled_macro_control_source_asts \
  --test compiled_macro_function_scopes -- --test-threads=2
```

Original code is informed by pinned analyzer.cljc2279–2300 and2304–2378.
No upstream forms were copied, inventory classifications changed, or Java/Node
dependencies added to shipped code. This bounded schema does not establish full
function/type/protocol annotations, source environments or inference. Public
compiler migration/evaluator retirement and original M3 lifecycle acceptance
remain required. Independent review, unfiltered full baseline and final-head CI
are required for the new function-name change before readiness.
