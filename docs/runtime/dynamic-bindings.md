# Rooted dynamic binding scope

This is bounded M2-02/M2-04 progress (issues #9/#11), not completion of the
portable core, compiled macro bootstrap, session frontend or async scheduler.

Original Rust HIR bootstrap handles `binding`, `with-redefs` and global `set!`.
Definition names retain `^:dynamic`/map metadata. Initializers run once in source order in the existing context. After all
initializers finish, the compiler snapshots selected outer values and publishes
the new values together through one frame. Duplicate targets use their last value;
reverse restoration preserves outer binding effects from the initializers. Failed initializers
publish no frame and keep prior arbitrary effects. Local assignment and missing
vars fail with source locations before session declarations publish.

Actual imported BindingCell identities select active values, so functions compiled
in earlier fragments observe the current context. Escaping functions read the
context at invocation; explicit lexical captures retain their captured value.
Nil and false remain real bound values. `set!` to an active target updates its
frame. `def` and `defonce` publish the real root, independently of an active frame;
reads inside that frame keep selecting its current binding. Scope exit restores
parent-frame entries without writing root cells, so root definitions survive
normal return, language throws and fuel recovery. Arithmetic macro calls keep
their compiled meaning while first-class arithmetic values use live cells.

## Runtime and recovery

ABI v1's existing Frame type stores a parent and private copied Args entries of
cell/original/current triplets. A rooted mutable runtime global retains the current
frame. Layout, manifest ABI version, compiler format and numeric dependencies stay
unchanged. Private helpers check storage shape/cell types; malformed frames raise
a typed language error. The input array is copied before publication.

Compiled body and cleanup closures use the universal Invoke ABI and typed
try-invoke. Normal return and language throws restore frames; cleanup exceptions
retain the established finally precedence. Region recurrence cannot cross the
scope boundary, while inner loops/functions remain legal.

Native eval/invoke/inspect boundaries retain a caller checkpoint. Fuel traps stay
traps; recovery owns pending values before restoring frames with private finite
runtime operations and restoring the operation's remaining fuel. Frame pop keeps
the child rooted until reverse writes finish, so interrupted restoration can be
retried idempotently. Recovery does not execute source finally callbacks. This is
not cancellation or asynchronous dynamic context support.

## Provenance and evidence

Reference: ClojureScript commit c4295f303100bbf5afac449242d30bca1126f1a1,
core.cljc with-redefs lines 2301–2327 and binding lines 2329–2340. Exact source
hashes/arities/dependencies are in docs/compatibility/reviews.edn. Upstream copyright
and EPL remain in the pinned submodule. No upstream form was copied; these original
intrinsics/bootstrap implementations are partial adaptations, not completed ports.

The original nineteen ClojureScript reference observations covered parallel/nested/duplicate bindings,
old/escaped functions and lexical captures, nil/false, throws/finally, snapshots
before initializer side effects, failed initializers, assignment, ordinary vars,
function redefinition and arithmetic macro/value separation. The fresh 278-case
portable source comparison and 16 actual pipeline tests passed again after interruption hardening. All 58 focused native tests pass,
including independent nested multi-target/duplicate fuel sweeps, escaped/thrown
closure retention across GC and native inspection callback trap recovery.
These are historical results. Their pre-initializer snapshot and definition
rollback expectations are superseded by pinned Clojure1.12.6 observations.
Current full-baseline/review/final-head CI results belong on the relevant GitHub
issue, with raw execution receipts in per-run `results/` files.

Executing regressions are crates/suss-cli/tests/portable_dynamic_bindings.rs and
crates/suss-compile/tests/portable_dynamic_bindings.rs. Initial six source tests
failed before implementation; an additional fuel regression exposed leaked active
values. A finer fuel sweep then exposed publication of the parent before restoration
finished (root 2 rather than snapshot 1 at fuel 430). Failures are retained in
[archived execution notes][execution-archive], rather than skipped or called successful.

The pinned analyzer warns for binding a var without dynamic metadata but still
executes it; this bootstrap permits the behavior but does not yet emit that warning.
Full source-backed macro expansion, metadata policy, field assignment, ExceptionInfo,
public Error classes and async context remain unfinished. Neither issue closes here.

## Clojure binding-effect correction

The pinned Clojure1.12.6 fixtures under
`results/m12-eval/binding-initializer-effects-primary` and
`results/m12-eval/binding-root-effects-primary` record nine initializer and eight
root-effect controls. They cover direct/evaluated definitions, previously unbound
roots, definitions during a binding body, `defonce`, nested outer-frame writes,
all initializers preceding snapshots, failures and ordinary `set!` restoration.
Prior successful native executions returned wrong values on three of five
initializer controls and five of six body controls. Those comparator failures
remain archived under `results/m12-eval/binding-effects-before`.

The native tests compare Runtime and isolated Macro hosts and actual compiled
macros to the primary. Production async-scheduler tests add a real pending await
and forced GC before checking the same binding/root effects. The unchanged fuel
sweep ranges check interruption recovery while preserving prior root definitions.
This correction does not turn the historical broader workspace results into
current-source validation or change the original suite's reviewed baseline.

## Root redefinition leases

`with-redefs` is a root operation. Its initializers run once in source order;
entry then captures actual roots and their bound bits before replacing them.
An active thread binding keeps priority. Definitions in the body change the
root immediately; normal return, exceptions and recovery restore only the
selected roots. Unrelated definitions/effects remain. `binding` keeps its
separate thread-frame semantics, including persistent root definitions.

The compiler carries the scope kind into synchronous and async IR. Root entry
uses nil old-value placeholders and captures the actual roots itself. A private
GC-owned binding wrapper records the real cell and old bound bit. The complete
storage and wrappers are validated before publication. The unwind frame is
rooted before the first root write; interrupted reverse restoration can retry
idempotently. Bound nil/false and an unbound root are distinguished by the bit.
No public value or Var is fabricated, and the shared frame/cell layout stays
unchanged. A pending async scope retains the globally visible root replacement
until its own return/cancellation. Forks omit root leases and copy thread binding
storage, so descendants cannot restore a parent's roots.

Twelve actual Clojure1.12.6 controls are retained under
`results/m12-eval/with-redefs-root-primary`. Both phases and actual compiled
macros compare them after GC. Native root-lease ownership, pending-await/cancel
and2000-step fuel tests pass with all earlier binding guards (318 focused tests
total). Earlier wrong-value and IR-admission failures remain source-qualified;
`verify-root-redefs-checkpoint.py` conserves the executed source and receipts.
Broader/full/review/final-head CI remain required on this compiler change.


Supplementary scope controls retain the original397-case ClojureScript corpus.
The native bootstrap currently permits root set! outside a thread binding;
Clojure1.12.6 rejects that spelling. The pinned legal outer-thread-binding
control in results/m12-eval/portable-dynamic-primary observes initializer effect2;
the exact original definition-in-binding source observes root9. Both original
native spellings and the two legal primary controls execute. This is an explicit
existing root-set extension, not a general Clojure error/compatibility waiver.
The module initialization trace observes binding-root-set with all original
order, partial-effect and module-publication assertions unchanged. Complete
portable22 and module11 targets pass; verify-portable-reference-checkpoint.py
relates these test-only deltas to the unchanged318-tested production source.

[execution-archive]: https://github.com/bobby/suss/blob/915be2fb1b8892b99a830d15bdf8a405e479c5f6/docs/roadmap/handoff.md
