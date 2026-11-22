# Bitwise and narrow casts

`oracle.clj` asserts30 JVM Clojure1.12.6 values, also executed in Runtime/Macro
and actual AOT. The fixture additionally has four Suss-only ADR-0004 replacement
controls; these are not JVM value matches. Source functions bit-clear/flip/set/test
observe redefinition. Genuine inline bit operations keep 64-bit raw values.
Checked float(long) rounds directly, while float(Object) has a double intermediate;
a private source-selected mode preserves both behaviors and checked replacements.

Known cast results remain raw even when an operand is boxed. Thirty-three
raw-return helpers share the checked conversion/coercion bodies and avoid numeric
result boxes. The library artifact regression inspects scalar signatures and
numeric boxer calls/struct construction. Three exact compiled loop components
exercise long bit operations, narrow integer casts and float casts.

The original before fixtures and failures are retained. The first bit-loop
fixture incorrectly required ordinary functions to inline; the final no-call
fixture covers genuine inline forms, and separate value/redefinition cases cover
ordinary functions. Initial raw-helper argument wiring and private-constant test
compile failures are retained separately. The first implementation passed2 CLI
and10 code-generation tests before the expanded helper artifact/rebinding checks.

`source-inputs.json`, `source.patch`, `source-check.py` reconstruct ten exact
production/test inputs from the recorded base. Final expanded gates pass74 focused tests across14 targets,
Java-free byte-exact bootstrap reproduction plus4 execution tests, actual
Binaryen133 optimization2, and reviewed-suite comparison1 with known failures
retained. Zero failed/ignored; the suite command filters10 unrelated harness tests.
Raw logs and complete compressed observations are retained. Do not
infer milestone/issue completion, current full-workspace or quiet timing evidence
from these bounded observations. No Java implementation is copied or shipped.

An additional probe retains six existing mismatches, plus two matching controls,
separately from the passing30-case matrix. Object byte/short/int conversion first
truncates; primitive float identity permits infinity, and int(float)'s upper bound
rounds to 2^31. The same8 checks execute in actual frozen-before/current AOT, both
2 passing/6 failing. These are required follow-up, never counted as successes.
`additional-overloads.*`, `additional-observations.json` and compile/run logs keep
the exact JVM/current evidence. The four ADR-0004 controls separately match frozen
before/current execution; component/compiler hashes are in rebind-observations.json.
