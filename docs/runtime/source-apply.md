# Retained source apply

M3 prerequisite: complete pinned `bounded-count`, `spread`, `list*`, private `next*`,
`apply-to-simple` and `apply` retain all signatures and branches. The bounded v1
bootstrap reproduces `gen-apply-to`/`gen-apply-to-simple`, including the 20-argument
ladder, language error above its apply-to bound, and larger source-array fallback.
Complete source generators, their `cs` dependency and EPL notice are extracted in
`docs/compatibility/bootstrap/apply-generators.cljc`; exact pin/hash checks and
`python3 scripts/apply_bootstrap.py --check` run in CI. This does not certify the
full compiled macro or Java-free bootstrap milestone.

Original compiler adaptation attaches actual callable signatures to general
closures. Compiled `applyTo` extracts the fixed prefix using live source first/next/
rest and passes the remaining persistent sequence directly to the variadic body.
It preserves unbounded/lazy tails instead of copying them into an argument array.
Canonical closure call/apply adapters ignore the JS this argument and invoke the
shared ABI. Nominal IFn properties resolve canonical phase keys; their call adapter
selects the actual method after arguments, while apply packs arguments beyond20
into a fresh source-array tail as pinned add-ifn-methods does. Ordinary source call
capture remains its separately established before-argument method selection.

Private source-array push copies storage, preserves receiver identity and returns
its new binary64 length. Signature tables and callbacks remain rooted by closures
and binding cells; layouts and ABI2 are unchanged. One canonical key table per
phase/protocol replaces repeated tables in generated source. It is initialized
once before a fragment executes. Bounded analyzer stack growth uses the existing
locked stacker0.1.22 dependency, with a hard64 expansion-depth guard; raising depth
alone overflowed the native test stack and was rejected.

## Execution evidence

Original source apply regressions:0passed/3failed. Helper regressions also failed
before retention; preserving private next* initially exposed unsupported attribute
handling. Qualified private access is callable in the pin, with a warning. Suss
retains declaration metadata; warning emission/namespace filtering remain open.

Fresh primary source apply:22 observations. Each isolated native Runtime/Macro
Store matches20 and separately asserts two accepted strict-arity boundaries: the
pin returns7 for an extra argument to a fixed function, and1 for extra arguments
to a vector; Suss evaluates operands then throws language errors, caught as111 and
47 respectively. These are explicit assertions, not matches or skips. Tests cover
25-argument invocation, fixed/multiple/variadic signatures, retained MetaFn/vector/
map/keyword/symbol calls, ordered effects, old captures/redefinition and GC.
An unbounded custom sequence proves exact fixed-prefix forcing (124 effect units)
and zero-fixed-prefix forcing (11), preserving the remaining sequence.

Five source apply tests and the30-test focused metadata/callable/control/named/
helper set pass. Four helper tests retain14 additional fresh primary observations.
The new ABI corruption guard passes, copying actual callback references into
foreign closures and asserting language exception tags/recovery after GC, including
empty push argument buffers. All14 existing compiler closure tests pass after
forward-declaring the new real seq dependency in their verifier fixture. Full ABI
suite did not execute: the shared target disappeared after the closure executable.
Python88 and inventory/import/provenance/WIT/numeric/offline checks pass.

The first key-table implementation used excessive generated code (3.3GB test RSS,
400% CPU); the live run was deliberately stopped, not counted as success. Sharing
the canonical table reduced the focused source apply run to4.2s. Required full
baseline, independent review/significant fixes and exact-final-head CI remain gates.

## Remaining M3 acceptance

Complete environments/source locations, syntaxquote/splicing/deterministic gensyms,
compiled bootstrap and macro phase dependency acceptance, reproducible versioned
Java-free artifacts, cache invalidation and evaluator removal remain open. Namespace
warning/filtering and complete frontend/core acceptance are also unfinished.
Stackless scheduler, pending-I/O cancellation, cleanup and live-GC counters remain
required. Original milestone issues12–15 stay open; no full M3 completion claim.
