# Scalar source AST facts in compiled macro environments

Metadata-free scalar initializer records in `&env` now expose `:op :const` and a
present `:val`, including nil and false. The value comes from the retained
analyzed source form through the rooted data bridge. Neither source execution
nor a physical HIR/runtime value determines this fact. Existing `:form`, `:env`,
inferred tags and `:suss/lowering` remain available. Scalar nodes have no
`:children` field, matching the pinned analyzer's observed field presence.

The five primary projections cover nil, false, 42, a string and a keyword.
`scripts/test-analysis-tags-oracle.sh` forces fresh pinned analysis, executes
its Node artifact, and checks an additional exact five-case trace alongside
all unchanged 32 tag observations and 11 executed projections. The initial
new checker failed because its candidate keyword encoding omitted the colon;
actual upstream data encodes `":const"` and `":word"`. The corrected fresh run
passes without weakening any existing assertion.

Both new native regressions failed on the parent implementation because
`:op`/`:val` were absent. After the repair, actual source macros inspect their
initializer ASTs in Runtime and Macro caller Stores after GC. The tests preserve
nil-versus-false-versus-absent distinctions, negative-zero bits, nonfinite numbers
and exact UTF-16 surrogate units. The additional effectful initializer executes
once and does not acquire a fabricated constant value field. These native edge
checks are separate from the five-case primary oracle scope.

The field contract follows pinned ClojureScript
`c4295f303100bbf5afac449242d30bca1126f1a1`, `analyzer.cljc`'s
`analyze-keyword` and `analyze-form` (lines1527 and4560–4600).
The source SHA-256 is
`297802c627474434f1ef868e31f5f9913c290a4e80c509a40c704dced95bbf47`.
The upstream copyright/EPL notice and [distributed license](../../runtime/core-import/epl-v10.html)
remain recorded. Native graph transport and fixture helpers are original code;
JVM/Node are development oracles and introduce no shipped dependency.

Metadata wrappers, quoted/compound nodes, variable/invocation operations,
children, declaration/function/method records and remaining inference rules
still need complete portable AST evidence. This increment does not certify the
whole `&env` schema or M3. Native host transport changed; compiler/runtime sources,
locked inputs and bootstrap bytes are unchanged. Independent review, the
unfiltered full workspace baseline and exact final-head CI are still required
for this new PR. See [the M3 audit](../roadmap/acceptance-m3.md) for the remaining
original requirements, including evaluator retirement and pending-I/O lifecycle.

Independent review preserves both original tests and adds executed checks for true,
a qualified keyword and escaped UTF-16 data, exact `:val`/`:form` agreement and
separate `:suss/lowering` presence. Local reads and arithmetic expressions yielding
scalar runtime results retain absent operation/value fields; this partial increment
does not invent their portable AST schema. All 38 affected native tests pass,
including the three constant tests in both caller phases after GC. An independent
fresh pinned run retains all 32 earlier observations and 11 executed projections,
plus the five scalar AST observations. No scoped significant defect was found.
Full reviewed-head baseline and exact final-head CI remain required.

The subsequent [local reference increment](compiled-macro-local-reference-asts.md)
adds genuine `:local` operation/declaration fields for resolved lexical symbols.
The earlier local-read test now checks that operation while retaining absent
constant-value and separate native-lowering assertions; arithmetic remains
unclassified. This supersedes only the earlier partial absence-of-operation
expectation for lexical reads.
