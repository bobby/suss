# Portable ExceptionInfo foundation

This is partial M2-02/M2-04 core progress (issues #9/#11). It retains the accepted
ClojureScript nil/throw contract; proposed ADR-0001 is not adopted.

Original runtime closures provide canonical live `ExceptionInfo`, `ex-info`,
`ex-data`, `ex-message` and `ex-cause` cells. `cljs.core` aliases `suss.core`.
Class values use a rooted descriptor and the existing UserObject layout; no shared
prelude, runtime ABI version or compiler format change is needed. A new private
rooted descriptor reserves its identity before later nominal allocations.

`ex-info` accepts two or three arguments, with nil as the omitted cause. Supplied
message/data/cause values remain unchanged, including false and closure values.
The source constructor ignores extra arguments and supplies undefined for missing
fields, as the pinned reference does. Universal invocation checks function arities
after ordered argument evaluation. Typed catches use actual descriptor identity;
same-shaped user objects cannot impersonate the original class.

Construction and ex-data/ex-cause predicates consult the current class cell, so
redefinition and dynamic scopes are visible to old getter values. Property reads
follow field names in the descriptor schema, including reordered fields and missing
fields. Missing properties produce undefined; nil and false are retained distinctly.
Storage/schema mismatches or malformed names produce typed language errors before
array access. Ex-message follows the original Error family rather than changing its
membership when ExceptionInfo's binding is replaced; generated runtime Error values
also expose their retained message.

Native bootstrap retains the class through its rooted Global when building dependent
closures. Escaped/thrown payloads and cause/data closures remain usable across frame
exit, GC and later fragments. The standalone source test host initializes the same
core cells and supplies the same class cell to dependent factories.

## Evidence and provenance

Reference: ClojureScript c4295f303100bbf5afac449242d30bca1126f1a1,
core.cljs11756–11823. Five public definitions have exact source hashes, arities,
dependencies and in-progress adaptations in docs/compatibility/reviews.edn. Upstream
copyright/EPL remain in the pinned submodule. No upstream form is copied; complete
extraction/patch/license packaging remains M4 work.

Nine original native regressions plus three independent review regressions plus the surrounding suites have executing evidence; the earlier complete surrounding focus passed66 before
adding the ordinary-call regression. New getters/class/GC/source order tests failed before implementation.
Live-cell factory initialization first exposed a scoped-root lifetime error; a
reordered-class regression then exposed positional rather than named field lookup.
Both failures and fixes remain recorded in handoff.

The fresh 303-case portable comparison adds25 ExceptionInfo observations to the
unchanged278 inputs; all16 actual decoded pipeline tests pass. Exact false values
are independently decoded directly; undefined numeric conversion is distinguished
from nil. Twenty additional primary exploratory observations retain ordinary
constructor/JS Error-parent behavior separately. Runtime ABI counter-forgery tests
are adjusted to forge the actual allocated numeric ID, preserving their identity
collision assertion as reserved descriptors grow.

## Remaining contracts

Persistent map data, complete Error class source surfaces/inheritance, printing and
stack/name/file adaptations, compiled core import and source field mutation remain
unfinished. Ordinary ExceptionInfo invocation in the pinned JS host can return a
host-global object; the bootstrap now returns a rooted per-runtime realm object, with distinct
identity and mutable raw fields, preserving truthiness/non-ExceptionInfo membership.
Three ordinary-call source cases passed fresh comparison; the full baseline remains required.
No source JS global-property interoperability is promised; broader ordinary-object
coercion and prototype/field behavior remain unfinished. Macro-phase hosts must provide their
own phase core cells; the compiled macro session remains M3 work.

Full workspace baseline, independent PR review and exact final-head CI are required
before readiness. This work closes neither issue by itself. Full portable exception
and core compatibility, production migration and M2–M9 remain incomplete.

## Selected arities and remaining JS wrapper adaptation

The two-argument ex-info implementation calls the live canonical ex-info binding
with a nil cause. Its escaped original three-argument implementation still constructs
using the live ExceptionInfo cell. Independent native invocation regressions retain
both behaviors through GC. The pinned development oracle's apply observations
confirm this distinction (two arguments call the replacement, three retain original
construction). Runtime factory initialization roots both class and self binding cells.

The pinned generated JavaScript generic wrapper also consults global arity slots:
a saved generic call can dispatch to the replacement's corresponding arity, and a
fixed-arity replacement lacking those JS slots can cause a host TypeError. These
raw observations are retained in the review handoff. The current universal callable
ABI selects captured implementations; generic JS wrapper/global-slot compatibility
is an unresolved adaptation, not a claimed passing source behavior. Full core callable
compatibility remains incomplete.
