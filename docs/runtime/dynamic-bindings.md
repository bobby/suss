# Rooted dynamic binding scope

This is bounded M2-02/M2-04 progress (issues #9/#11), not completion of the
portable core, compiled macro bootstrap, session frontend or async scheduler.

Original Rust HIR bootstrap handles `binding`, `with-redefs` and global `set!`.
Definition names retain `^:dynamic`/map metadata. All original target values are
read before any initializer, then initializers run once in source order. Values
publish together through one frame. Duplicate targets use their last value;
reverse snapshot restoration preserves the original value. Failed initializers
publish no frame and keep prior arbitrary effects. Local assignment and missing
vars fail with source locations before session declarations publish.

Actual imported BindingCell identities select active values, so functions compiled
in earlier fragments observe the current context. Escaping functions read the
context at invocation; explicit lexical captures retain their captured value.
Nil and false remain real bound values. Assignment/definition to an active target
updates its frame; scope exit restores the snapshot. Arithmetic macro calls keep
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

Nineteen new reference observations cover parallel/nested/duplicate bindings,
old/escaped functions and lexical captures, nil/false, throws/finally, snapshots
before initializer side effects, failed initializers, assignment, ordinary vars,
function redefinition and arithmetic macro/value separation. The fresh 278-case
portable source comparison and 16 actual pipeline tests passed again after interruption hardening. All 58 focused native tests pass,
including independent nested multi-target/duplicate fuel sweeps, escaped/thrown
closure retention across GC and native inspection callback trap recovery.
Full-baseline/review/final-head CI results belong in handoff.

Executing regressions are crates/suss-cli/tests/portable_dynamic_bindings.rs and
crates/suss-compile/tests/portable_dynamic_bindings.rs. Initial six source tests
failed before implementation; an additional fuel regression exposed leaked active
values. A finer fuel sweep then exposed publication of the parent before restoration
finished (root 2 rather than snapshot 1 at fuel 430). Failures are retained in the
handoff, rather than skipped or called successful.

The pinned analyzer warns for binding a var without dynamic metadata but still
executes it; this bootstrap permits the behavior but does not yet emit that warning.
Full source-backed macro expansion, metadata policy, field assignment, ExceptionInfo,
public Error classes and async context remain unfinished. Neither issue closes here.
