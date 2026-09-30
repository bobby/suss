# Arithmetic function values

The portable pipeline now supports +, -, * and / as values, including higher-order,
computed and returned callees. They read canonical live binding cells; suss.core
and cljs.core share the same runtime identity. Repeated reads return the same
function object until replacement. An old captured function retains its behavior
after rebinding and GC. Source cells remain isolated by phase.

Native Session provisions four runtime-phase arithmetic cells once per Store.
SessionStats.binding_cells includes these four; reset reprovisions them. Prepared
fragments record materialized arithmetic cells in their staged environment without
changing the caller's environment. Other embedding hosts can use
Environment::arithmetic_bindings(phase) to provision the correct initial functions,
then reuse cell globals. This native API does not implement compiled macro sessions.

Runtime factories arithmetic-add/subtract/multiply/divide produce universal shared
Invoke closures. Minimum arity is zero for +/* and one for -//; maximum -1 is
variadic. Zero-argument +/* yield zero/one; unary +/* retain exact value identity;
unary - negates and unary / reciprocates. Remaining evaluated arguments fold left
to right using checked primitive arithmetic. Caller evaluation of the callee and
arguments happens once in source order, before invocation. Central invoke raises
the shared typed Wrong arity exception. Internal invoker exports are trusted ABI
helpers, not public source functions.

The invoke bodies use the actual recursive prelude Invoke type, rather than a
new standalone type with the same signature. Declarative elements permit ref.func.
The ten-type prelude and ABI version remain unchanged; new factory exports are
additive and actual linking rejects absent exports before eval.

The 176-case source corpus includes 18 newly executed pinned ClojureScript
function-value cases. Independent GC tests cover closure min/max fields, exact
number bits after GC, zero/unary/ordered variadic behavior and exception tag/
descriptor. Session tests inspect reference identity, original captures after core
rebinding, alias lookup, effects and compile-error isolation. Phase fixtures
supply different functions to separate runtime/macro cells and execute both.

Provenance: original Suss implementation, informed by pinned ClojureScript commit
c4295f303100bbf5afac449242d30bca1126f1a1 core.cljs lines 2724–2753, plus arithmetic
macros in core.cljc. No upstream forms are copied. Four runtime declarations now
have manual public/arity/dependency classifications as adapted and in-progress;
1,061 declarations remain unassessed. Neither this classification nor bounded
numeric cases certifies complete public core compatibility. Existing ryu-js/Rust
helper notices remain unchanged; no dependency or shipped JVM/Node path is added.

Direct arithmetic syntax still uses the bounded intrinsic/macro bootstrap when
resolution selects it. Source-backed compiled macro expansion remains unfinished.
A development with-redefs probe showed the upstream generated JS wrapper can read
a replaced global's arity property when an old core function is called; that exact
TypeError is recorded in handoff, not adopted over the accepted old-capture contract.
Object conversion, complete macro/core import, extended signatures, collections,
ExceptionInfo/recur/effects and production command/REPL migration remain open.
This increment uses Refs #9/#10; full acceptance is not complete.
