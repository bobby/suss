# Source-local reference ASTs

This increment targets initializer ASTs whose retained source is a resolved
lexical symbol. The declaration recorded by `SourceAnalysis.resolved` supplies
`:info`, `:name`, `:local`, and any actual `:arg-id`, `:variadic?` and `:init`.
The source operation is `:local`; neither the initializer's runtime result nor
its physical lowering determines that operation. Existing `:form`, `:env`,
source tags and `:suss/lowering` remain separate facts. No `:val` or `:children`
field is invented for a lexical read.

The targeted cases are a let local, a shadowed local, a loop local, a fixed
argument, a rest argument and a named function self-reference. Projections
compare field presence and exact values, declaration/initializer equality and
identity. The native regression runs source macros in both caller phases after
GC, and checks that an effectful initializer still executes once. Fresh force/cache-disabled pinned compilation and Node execution verified all
six observations exactly. The native parent regression failed because operation,
local kind and declaration info were absent. After the repair, all39affected
native tests pass across7groups, with0failures/0ignores/0filtered. A separate
negative regression checks an effectful invocation through a local callee: its
initializer must not be classified as the callee reference. That extended test passes in both caller phases after GC (1passed/0failed/
0ignored/0filtered). All118existing Python checks also pass.

The contract comes from ClojureScript
`c4295f303100bbf5afac449242d30bca1126f1a1`, `analyzer.cljc`'s
`analyze-symbol` (lines4100–4120), alongside actual binding construction and the
named-function second pass. Source SHA-256:
`297802c627474434f1ef868e31f5f9913c290a4e80c509a40c704dced95bbf47`.
Upstream copyright/EPL provenance and the
[distributed license](../../runtime/core-import/epl-v10.html) remain recorded.
The projection helpers and native transport are original code; JVM/Node remain
development tools with no shipped dependency.

Global/field references, quotes, compound/invocation nodes, logical children,
complete declaration/function/method/inference schema and evaluator retirement
remain unfinished. This is partial progress for issue14, not complete `&env`
or M3 acceptance. Full reviewed-head baseline and exact final-head CI are
required before this increment is ready.

Independent review reruns the unchanged six-case pinned oracle and adds executing
queries for initializer-scope identity after later shadowing, distinct declaration
and reference metadata, variadic named self-reference and catch-alias locals.
Both caller phases pass after GC, together with existing scalar/binding/source-tag
checks. No production repair was needed in this scoped review; complete source
AST and original M3 requirements remain open.
