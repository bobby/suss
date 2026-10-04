# Control source AST evidence

Issue #14 now has executing evidence for original control operands, binding
initializers and function methods retained before lowering. Fifteen selected
pinned observations agree with native compiled macro projections in both caller
phases after GC, with the explicit private catch-name correspondence below.
This establishes the selected source fields and declared child edges. Complete
AST schemas/inference, public compiler migration and evaluator retirement remain
required for Milestone M3.

## Source facts

| Operation | Declared children | Retained analysis |
| --- | --- | --- |
| `:do` | `[:statements :ret]` | Ordered expressions, including an analyzed implicit nil result |
| `:if` | `[:test :then :else]` | Original condition and both alternatives, including implicit nil |
| `:let`, `:loop` | `[:bindings :body]` | Real declarations and once-analyzed initializers; synthetic analyzed `do` body |
| `:recur` | `[:exprs]` | Original replacement operands before edge assignment |
| `:fn` | `[:methods]` | Original methods before physical duplicate-arity elimination |
| `:fn-method` | `[:params :body]` | Real parameter records and analyzed synthetic `do` body |
| `:throw` | `[:exception]` | Original exception operand |
| `:try` | `[:body :catch]`, with optional `:finally` | Analyzed body, generated handler syntax and optional cleanup |

The bounded bootstrap actually expands simple `let`/`loop` forms into
`let*`/`loop*` before source facts are captured. Bare `let*` is a primitive special
form; a same-named lexical value does not replace it. Initializers run once in
textual order. Unsupported destructuring remains a located error.

Function method records retain exact explicit method forms, reader metadata,
parameter declaration identities and variadic syntax before runtime normalization.
The actual source body is captured before its compiler-owned recurrence wrapper;
it is not recovered from that wrapper. Source methods include duplicate arities
before runtime elimination. Metadata keeps separate wrapper and inner function
edges. The follow-up evidence below covers selected method entry-environment fields,
body markers and accepted recurrence facts. Method type annotations, complete
environment schemas and broader inference policy remain open.

`loop`/`recur` execution continues to use IR jumps and Wasm branches, without
recursive stack growth. Tail position and arity are checked; all replacements
are evaluated before assignment. These new records expose that source analysis
without changing it into general automatic tail-call optimization.

Try analysis visits finally, catches, then body, matching pinned analysis-time
macro visitation. Runtime still executes body, selected handler, then cleanup.
Generated handler syntax is analyzed before closure packaging. A typed catch
uses canonical `cljs.core/instance?`, independently of a lexical same-named value.
The catch alias retains its actual private-local initializer and distinct source
declaration identity while sharing the physical payload ID. Ordinary nested
bindings retain separate IDs. A bare try retains the analyzed synthetic
`(throw nil)` source fallback; an absent runtime handler remains absent.
Initialized binding records retain present nullable tags; unknown use-site local
tags remain absent.

Compiler-generated body/primitive rewrite frames use the existing native stack
growth guard. They do not charge an extra source nesting level. Source and
arbitrary macro expansion retain their unchanged 64-frame bounds.

## Primary and native comparisons

The development oracle executes revision
`c4295f303100bbf5afac449242d30bca1126f1a1`. Its bounded projection follows actual
`:children` edges and records operation/tag/form presence, literal flags and
one/many child cardinality. It neither reanalyzes forms nor serializes whole
binding/environment maps. Fresh analyzer traces equal executed Node output
exactly for all fifteen cases, including final effects `2`.

The initial raw catch observation contains `e16497`. A fresh compilation yielded
`e15746`; its analyzer trace and Node output agree exactly. This compiler-private
name depends on the upstream process's gensym counter. The frozen corpus stays
unchanged. The primary checker creates a separate comparison view, validates the
numeric `e` prefix and exactly two matching payload uses, and requires every
other field to match. Analyzer-to-Node comparison remains raw and exact, even for
these names. Inconsistent names, extra uses and user symbols fail.

Native Suss uses its own deterministic `$exception` name and binding counter.
The native comparison validates the selected default catch's original `let*`
form, numeric private-name suffix and exactly two consistent uses before making
one explicit correspondence in a copy of the reference row. All other fields
remain exact. This is an alpha correspondence, not an exact spelling match,
blanket normalization or skipped case. Separate compiler/native regressions
assert private payload identity, collision avoidance, shadow restoration and
actual caught values.

```sh
sh scripts/test-control-source-asts-oracle.sh
python3 -m unittest discover -s scripts -p 'test_control_source_asts_oracle.py'
cargo test -p suss-compile --test portable_control_source_analysis --locked -- --test-threads=2
cargo test -p suss-cli --test compiled_macro_control_source_asts --test compiled_macro_binding_records --test portable_let_star --locked -- --test-threads=2
```

The checker has eleven positive/negative checks. Native tests independently
decode actual rooted results after GC in both phases, including cleanup effects,
function metadata and malformed alpha correspondences. Compiler callbacks prove
once-only expansion, original declaration identity, context and bounded depth.
The primary comparison alone does not prove native behavior.

## Provenance and remaining gates

The original implementation is informed by pinned `cljs/analyzer.cljc`:
`parse-try`/`parse-throw` at 1890–1984, function methods at 2208–2352, and binding
analysis at 2483–2594; primitive binding expansion follows `cljs/core.cljc`
772–813. No upstream forms are newly copied. Existing adapted parameter policy
keeps its EPL notice and pinned provenance. Java/Node remain development oracles.

Parent regressions failed for primitive `let*`, conditional children, binding
forms/children, recurrence, function methods and try regions. These failures and
an actual bootstrap stack overflow are retained in the handoff with the repairs
and terminal results. Both bootstrap phase pairs reproduce without Java.
The unfiltered workspace baseline, independent PR review and final reviewed-head
CI remain mandatory before readiness. No issue or milestone is closed by this
bounded corpus. Public frontend/evaluator retirement and session pending-I/O,
cancellation and live-memory acceptance remain open.


## Method entry environment and recurrence facts

Method records now capture their actual entry environment before allocating
parameters: enclosing locals, fields, namespace/catalog, function scopes and
analysis context. The body retains its own return context and parameter identities.
A shadowing parameter does not replace the enclosing local in the method entry
snapshot. Multiple methods retain expression context even when the enclosing
function is analyzed as a statement. These are captured facts, not environments
reconstructed from body locals.

The actual analyzed synthetic body is marked with present true `:body?`;
ordinary operands and generated handler roots keep that field absent. The existing
body-analysis helper marks function, binding, try and finally regions before
physical closure/control lowering. Compiler regressions distinguish these roles.

A method's present nullable `:recurs` records accepted recurrence to its own
analysis target. An inner loop or inner function does not mark the enclosing
method. Unselected branches still contribute analysis facts. Arity and tail
position errors remain errors; this record changes no runtime control flow.

Seven fresh pinned analyzer observations, containing eight method records, equal
raw executed Node output exactly. All seven functions execute to42. The shared
`method-recurrence-observations.json` is frozen from that actual output. Native
compiled macros compare the same corpus in both caller phases after GC and execute
the same functions. The selected projection checks recurrence field presence/value,
body marker presence/value, entry context and absence of the not-yet-allocated x
parameter. Separate compiler regressions check the actual outer-local/parameter
shadow identities and statement versus multiple-method contexts.

```sh
sh scripts/test-method-recurrence-oracle.sh
python3 -m unittest discover -s scripts -p 'test_method_recurrence_oracle.py'
cargo test -p suss-compile --locked --test portable_control_source_analysis -- --test-threads=2
cargo test -p suss-cli --locked --test compiled_macro_control_source_asts -- --test-threads=2
```

The original probe follows pinned `analyzer.cljc`2241–2277; no analyzer code or
new core forms are copied. Its strict checker rejects false instead of nullable
recurrence, missing fields/cases, raw analyzer/Node disagreement, nonnumeric
execution results and corpus variance. Seven transport checker tests pass; they are
separate from upstream/native semantic evidence. The parent native regression
returned42 but failed for absent recurrence/body markers and method environment.
The implementation passes11 compiler checks and the four-test native control
suite, including the shared corpus and previous fifteen-case observations.
Both bootstrap phase pairs reproduce byte-for-byte without Java; all four bootstrap
tests pass. The complete168-test Python suite passes; inventory1065 and overlay
371reviewed/694unassessed remain unchanged.

This selected corpus does not establish every method environment field, method
type annotation, named function child schema, inference or evaluator retirement.
Independent review, unfiltered workspace baseline and final reviewed-head CI are
still required for this follow-up. M3 issues12–15 remain open.
