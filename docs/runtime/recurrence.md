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
replacement and GC. Both loop recurrence and function-level recur use a backedge
rather than recursive invocation, so iterations do not grow the call stack. This
guarantee does not extend to ordinary recursive function calls.

Ordinary closure calls also receive a tail-transfer optimization when verified IR
forwards their result unchanged through empty blocks to a return. The already
evaluated callee and arguments pass to `return_call`; runtime invocation and
fixed/variadic closure dispatch use tail transfers too. This preserves live global
lookups and captured old functions rather than rewriting self calls to `recur`.
An enclosing `try`/dynamic-scope helper retains its pending cleanup frame;
non-tail calls and per-recursion pending cleanup are not discarded. This is an
optimization at eligible call sites, not a blanket guarantee for ordinary
recursive source calls.

Lowering represents terminating recurrence with no result. An if joins only arms
that produce values; two recurring arms produce no unreachable join or fabricated
nil. Public HIR validation independently checks lexical target, arity and tail
positions. IR dominance, parameter shape/type and reachability checks still run
before actual Wasm validation and execution. The current shared ABI is version 2;
recurrence uses the existing shared Invoke type without a recurrence-specific ABI. Infinite zero-binding/function recurrence produces a distinct host fuel
trap; a subsequent native Session input executes normally.

Evidence includes the original failing source regression, executing compiler
negative/public-HIR tests, native parallel/capture/effect/type/fuel tests, and 18
fresh pinned recurrence observations in the 194-case common source corpus. Corpus
values are independently decoded from actual Wasm after GC. These checks do not
certify destructuring, extended function signatures, general exception/effect/
suspension control flow, compiled macros, full core import or frontend migration.
Public Compiler execution regressions also run 100,000 loop iterations and
100,001 function recurrences with a 2 MiB Wasm stack limit, independently checking
typed results and simultaneous replacement. These are
`public_compiler_loop_recur_executes_deep_iterations_with_bounded_stack` and
`public_compiler_function_recur_uses_bounded_stack_and_simultaneous_rebinding` in
`crates/suss-compile/tests/public_compiled_pipeline.rs`. Keep issues #9/#10 and
M2–M9 open until their complete acceptance gates pass.
