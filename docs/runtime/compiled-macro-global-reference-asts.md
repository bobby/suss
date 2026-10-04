# Resolved global source AST facts

Initializer records for genuine resolved global symbols expose `:op :var`,
qualified `:name`, canonical `:ns`, and `:info` copied from the captured
`SourceBinding::Global` declaration revision. Resolved operation/name/namespace
override raw declaration metadata in this copy. The namespace catalog retains
its original record, including raw metadata overrides. Global AST tag presence
and value come from resolved info, including false; lexical inference is unchanged.
Runtime values, physical HIR and source reexecution do not determine these facts.

Eleven fresh pinned observations and all eleven executed Node projections cover
scalar/qualified/hinted/dynamic/function/declared references, raw metadata overrides,
core aliases and old/new declaration revisions. The native parent regression
reproduced absent var identity/info fields. The initial new native helper also
exposed unavailable public `symbol`/`mapv` functions; the bounded projection now
receives the unqualified declaration key as a raw macro argument and explicitly
selects the same fields in both implementations. Fresh force/cache-disabled
analysis and actual Node execution revalidate every unchanged expected observation.
This establishes the projection, not support for those unimplemented core APIs.

The repaired graph initially failed the core case because the identity adaptation
patch omitted its upstream docstring. The patch now retains the original body and
docstring in the actual compiled declaration. Source extraction/hash, patch hash,
review overlay and generated manifest retain provenance and the EPL notice/license.
Both Runtime and Macro bootstrap image pairs were regenerated; Java-free
reproduction and all four executing bootstrap tests pass before native acceptance.

All 58 affected native tests pass across nine groups (graph11/bindings8/scalars3/
globals1/locals2/env3/tags2/syntaxquote11/core-import17). The eleven global cases run
in Runtime and Macro caller Stores after GC, with only the exact two core resolved
name/ns spellings adapted to the design's canonical `suss.core`. Old declaration
metadata remains captured after redefinition, while the live value changes; the
initializer effect occurs once. The extended focused test also passes: a global
callee invocation does not inherit its callee's var identity or a fabricated
constant value, and its effect occurs once. All123Python checks pass, including
five observation-corruption checks that distinguish numeric1 from true metadata.

The source contract follows pinned ClojureScript
`c4295f303100bbf5afac449242d30bca1126f1a1`, `analyzer.cljc`'s
`analyze-symbol`4149–4165, `resolve-var` and default `resolve*`1248.
Analyzer SHA-256:
`297802c627474434f1ef868e31f5f9913c290a4e80c509a40c704dced95bbf47`.
Identity source SHA-256:
`4997a405f43df040b92909ac831f031385fd1ac86948cb1be06dd3529323504e`.
[Identity adaptation](../compatibility/patches/identity.json),
[generated manifest](../../runtime/core-import/manifest.json), and
[EPL license](../../runtime/core-import/epl-v10.html) remain distributed.
Native graph transport and observation helpers are original code; JVM/Node remain
development oracles with no shipped dependency.

Field/quote/compound/invocation/children ASTs, constant-expression policy, complete
source declaration/function/method/inference schema and evaluator retirement remain
unfinished. Dependency/target integration and rooted pending-I/O cancellation plus
code/live-heap accounting remain original M3 gates. Independent review, significant
fixes, the unfiltered full workspace baseline and exact final-head CI are required
for this increment. No issue closure, merge or milestone completion.
