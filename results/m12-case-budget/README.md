# Case integration CI repairs

CI45158952814 on2254f70c exposes four failing integration groups; their complete
job logs are retained in `results/m12-case-budget`. The original record-budget
failure also reproduces with a copied, hash-pinned native CLI. Its unrepaired
full workspace run is explicitly interrupted after1271.7s with3877 recorded
inputs unchanged, terminal exit-2; it is never counted as a pass.

The compiled case macro now constructs its finite dispatch tree eagerly,
avoiding lazy syntax concatenations forced by the macro bridge. The unchanged
four-million record budget passes, with namespace-membership costs963185 and
975395 below the original one-million limit. Literal hashing, collision callbacks,
duplicate guards, effects, lexical capture and tail recurrence remain required.

Fresh pinned JVM traces replace the old sequential-public-equality assumptions
in the mixed-case tests. Unique buckets make no public equality calls; the nil/0
bucket retains its exact callbacks and a real ExceptionInfo throw, GC-rooted
message/data, binding restoration and recovery. Duplicate diagnostics retain exact
source spans and compiler context around the actual JVM root messages. An old
when-let test now checks the extracted body messages instead of requiring an
opaque uncaught exception. Both JVM spec-precheck and direct macro-body messages
are recorded separately; the native source adaptation uses compiler spans in
place of the JVM namespace suffix. Original tests and all failures remain retained.

Tooling emits letfn nodes as letfn* with complete binding initializers. A new
mutual-recursion/arity/capture/typed-parameter regression also exposes physical
parameter conversions leaking into source methods; source facts now retain the
original body while executable HIR keeps its casts. Core round trips pass in both
phases, with no lowered nodes. Macro-source and primitive-hint execution checks
pass too.

`run-gates.py after-source-body-before-primitive-casts` passes ten groups with106
reported Rust test executions; the separate compiled_when_let command passes five
more after its test-only update. These include all72 vendored case assertions,
328 Runtime/Macro/actual-AOT controls, independent GC/privacy, complete component
provenance, unchanged AOT limits2352 <=3900 and96 <=128, and Java/Node-free
bootstrap reproduction/execution. All recorded gate inputs remain unchanged.
`run-foundation.py foundation-after-when-let-provenance` passes all15 checks including364 Python tests. The native fixture adaptation patch is refreshed from its unchanged captured source; the original patch remains retained.
Primary data, failure baseline and original budget/size tests stay byte-identical.
The repaired unfiltered workspace run, final-head CI and independent promotion
review remain separate required evidence. Commands and scope qualifications are in
`results/m12-case-budget`; no full BigDecimal acceptance follows from these fixes.

