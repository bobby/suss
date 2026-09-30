# Portable source recurrence

The replacement pipeline lowers simple-symbol `loop`/`loop*` bindings and fixed
function `recur` to explicit IR header parameters and backedges. `loop` is a bounded
bootstrap macro with normal lexical shadowing and canonical suss.core/cljs.core
resolution. `loop*` and `recur` are true unqualified special forms. This is an
original compiler implementation, not copied upstream core source. Semantic
provenance is pinned ClojureScript c4295f303100bbf5afac449242d30bca1126f1a1,
core.cljc lines 789–811 (EPL-1.0 upstream), and its compiler's recurrence contract.
The complete loop macro, including destructuring, remains unassessed in the inventory.

Initializers execute once in source order, with later initializers seeing earlier
bindings. The body establishes the nearest lexical recurrence target. Recur is
legal only at its tail positions: the final body/do form, alternatives of a tail
if, or the body of a tail let. Callees, arguments, conditions, definition and
binding initializers are value positions. Outside-target, non-tail and wrong-arity
recurrence produce source diagnostics. Function boundaries establish separate
parameter targets, including zero parameters; a function cannot recur an outer loop.

All replacement expressions are evaluated before header parameter assignment.
The existing edge emitter pushes every value before writing any parameter local.
Header parameters have conservative Value types because replacement may change
Number, String, nil or closure identity. A synthetic loop in each fixed function
body preserves incoming parameters and immutable outer captures. Closures created
in an earlier iteration capture that iteration's values, surviving subsequent
replacement and GC. Recurrence uses a backedge rather than recursive invocation.

Lowering represents terminating recurrence with no result. An if joins only arms
that produce values; two recurring arms produce no unreachable join or fabricated
nil. Public HIR validation independently checks lexical target, arity and tail
positions. IR dominance, parameter shape/type and reachability checks still run
before actual Wasm validation and execution. ABI v1 and its shared Invoke type are
unchanged. Infinite zero-binding/function recurrence produces a distinct host fuel
trap; a subsequent native Session input executes normally.

Evidence includes the original failing source regression, executing compiler
negative/public-HIR tests, native parallel/capture/effect/type/fuel tests, and 18
fresh pinned recurrence observations in the 194-case common source corpus. Corpus
values are independently decoded from actual Wasm after GC. These checks do not
certify destructuring, extended function signatures, general exception/effect/
suspension control flow, compiled macros, full core import or frontend migration.
Keep issues #9/#10 and M2–M9 open until their complete acceptance gates pass.
