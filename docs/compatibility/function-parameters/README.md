# Portable function parameter lowering — authoring checkpoint

This implements the pinned `maybe-destructured`/`destructure` binding graph at the
compiler's bootstrap `fn` boundary, including named/multiple/variadic signatures
and method entry. Primitive `fn*` retains its symbol-only parameter contract.
Each pattern receives a hygienic raw argument. An inner sequential `let*` performs
extraction inside the existing method recurrence loop, so `recur` replaces raw
arguments and executes extraction again. Captures remain genuine compiler bindings.

Vector patterns retain nested extraction, missing `nth` defaults, rest via
`seq`/`first`/`next`, the pinned advancement-before-nested-extraction order, `:as`
and empty-tail nil. Map patterns retain sequence normalization, alias binding,
nested patterns, qualified keys/syms/strs, unqualified local identities and eager
`get` defaults even when a key exists. All runtime operations use live canonical
source cells. Syntax map planning retains array-map promotion, HAMT encounter
order, stable hash collisions and persistent ordering after removals. Numeric
syntax data uses the accepted binary64 portable contract.

The five complete macro parent forms, reader contexts, hashes and upstream EPL
notice are retained here. This is Rust source-aware binding lowering, not a claim
that the retained guest macro graph now executes. Existing pre/post-condition,
general `let`/`loop` pattern lowering and full nominal/reify implementation remain
separate compiler dependencies.

Six whole runtime prerequisites are appended after all 350 existing selections:
`LITE_MODE`, `to-array`, `--destructure-map`, `reverse`, `vector`, `some`.
`--destructure-map` preserves both branches, including odd/trailing-map sequence
normalization; it is not replaced with `apply hash-map`. The pinned default mode
is false. Full ObjMap/Closure build-time LITE_MODE override remains pending.
`reverse` keeps reversible and reducing paths; `vector` keeps the actual IndexedSeq
fast path/fromArray alias flag and vec fallback; `some` returns the first truthy
predicate value, without Booleanizing it. Its exact when-let is expanded through
one temporary/when/inner-let. To-array retains every operation, adapting immutable
nil? and the exact canonical `. ary push` spelling to `.push ary`.

## Author checks

- `sh scripts/test-function-parameter-oracle.sh`: fresh pinned 39 raw observations
  match exact tags, numeric bits, UTF-16, ordered values and effect traces.
- `python3 -B scripts/core_import.py --check`: 360 import files verified.
- `python3 -B scripts/check_function_parameter_provenance.py`: five whole parents.
- `python3 -B scripts/check_function_parameter_runtime_provenance.py`: six exact
  whole runtime adaptations.
- `python3 -B -m unittest discover -s scripts -p 'test_*.py'`: 269 passed, 21.278s.
- Rustfmt parsed/formatted the new Rust modules/tests; `git diff --check` passed.

The closed shared corpus includes >8 promotion, nested vector/map pattern keys,
independently observed symbol hash collisions and reversed insertion, removals,
24-key promotion, nonselected eager-default throws, recurrence, live public calls
and captured defaults. The native test independently decodes every result after
GC in both phases; a separate sole-host callable test retains captured vectors
and invokes it repeatedly across GC. Those tests require final frozen-head execution.

## Retained diagnostics and limits

The parent compiled an isolated authoring snapshot successfully, then advanced the
complete bootstrap through real dependency failures: reverse, vector, some,
canonical dot spelling and when-let. These failed runs are retained, not passes.
After complete dependency adaptations, isolated bootstrap generation exited zero.
The 30-case isolated native gate passed two tests; a later 39-case snapshot passed
two tests in 8.05s. Their exact source hash receipts are retained. They certify
those snapshots only, not this final authoring projection.

The 39-case compiler-library diagnostic was 91 passed/2 failed: the new structural
HIR fixture lacked the canonical Keyword cell, and an older-base sparse fixture
still used dense constructor sizes. Applying the already published sparse fixture
repair and a Keyword declaration produced 92 passed/1 failed, exposing the missing
PersistentVector structural setup. The final authored fixture declares genuine
canonical Keyword/PersistentVector/PersistentArrayMap source cells and inspects the
real method-body Do wrapper before the inner Let. It supplies no fake class body.
The parent rejected a mixed latest-module copy when its production projection
changed; that attempt is not claimed as execution of the final source.

All four author checkout bootstrap files are unchanged and stale. The published
06ac runtime/fixture repairs must be inherited during final integration; this
checkpoint does not edit or weaken the older sparse test. No author Cargo,
bootstrap build or push occurred. Independent review, final-head compiler/native
execution, bootstrap reproduction, unchanged full baseline and CI remain required.
This is a prerequisite for Refs #18/#19, not record/reify or M4 completion.
The next source group is genuine retained reify plus nil-iter, including the
empty-HAMT iterator path; no replacement class or protocol subset.
