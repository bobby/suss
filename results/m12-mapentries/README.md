# Canonical MapEntry scalar storage

The aggregate pass now admits a locally constructed MapEntry used only by
constant in-bounds indexed projections. Its key, value and hash expressions run
once in source order. Canonical constructor/schema admission lets the scalar
continuation remove construction and its public construction guard; misses run
the original guarded constructor and lookup. Generic SSA types still select
source Object casts. Private vector construction is never granted that public
exemption. Escapes, unknown indices and normalized async/future plans retain the
conservative path; the shared copy allowance still derives from original size.

Core registers one MapEntry witness in its existing original-Suss bootstrap
extension. The new private descriptor global is appended after existing runtime
globals; existing indices and the shared type group stay unchanged. Native
indexed reads accept only that registered descriptor, a three-field owner and
indices0/1. The registry must hold a descriptor; absent/malformed layouts miss.
Admission caches successful immutable schemas while rechecking current canonical
registration on every invocation. Core import source, recipe hashes and manifest
are regenerated and `core-import-check.log` reproduces them.

Sixteen fresh JVM Clojure1.12.6 assertions cover wide/mixed values, Object
conversions after projection, NaN/-0.0/overflow, ordered effects/throws, root
rebinding, escaped entries, closures and atom identity. The JVM factory adapts
the portable three-field MapEntry constructor; cache-field body code is not
ported. Source pairs, replacements, exact forms and assertion logs are under
`oracle/`. Existing Clojure source/JAR/EPL provenance is retained with the parent
inline-nth receipts. No Java dependency is added to the shipped compiler.

The combined source, rebased on9596dce6, passes9 tests with no failures/ignores:
MapEntry and vector Runtime/Macro/AOT execution, the full39-case parameter
corpus in both phases, and3 IR/encoding checks. The earlier isolated execution
also passes5 tests. These snapshots remain separate. `source-check.py`
reconstructs14 production/test/recipe/generated inputs from9596dce6 and the
patch, and checks current bytes. The initial source snapshot predates that
rebase and remains explicitly archived. Initial stale import-hash failure and
the corrected generated-source check are retained.

`measure.py` compares the frozen9596dce6 executable with this combined source
on identical four-case benchmark programs. All32 RSS and16 perf sums plus16
warm checks pass. Two null-GC RSS slopes at1M/2M iterations estimate MapEntry
allocation at135.12/136.37 bytes per iteration before, below resolution after.
User-space instruction deltas at100K/200K fall5578.89 to179.98. Vector remains
below RSS resolution at about449 instructions; control/closure remain below
resolution at about28. Actual compiler/component hashes, commands and outputs
are retained. These are allocation/instruction observations, not quiet timings.

`run-gates.py` selects37 focused targets, then Java-free bootstrap reproduction,
Binaryen133 and reviewed-suite comparison. It is active; inspect terminal
statuses before treating the broader run as passed. Final-head CI remains a
separate gate. One aggregate per function, unsupported letfn, the existing NaN
identity difference and unfinished numeric-tower work remain explicit limits.
