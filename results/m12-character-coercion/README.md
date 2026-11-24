## Character coercion source overloads and diagnostics (2026-11-23)

`run-gates.py character-float-format-and-legacy-guards` passes103 tests in five
bounded groups, all recorded source/fixture inputs unchanged: character and phase
observers11, original numeric casts19, namespace/source tooling56, original
record/calling-convention/size guards16, actual linker-failure rollback1.
The character matrix executes369 controls in Runtime, Macro and real CLI AOT:
365 primary matches and four explicit accepted platform differences per route.
Seven additional effect/arity controls execute in both phases and actual AOT;
retained code units are independently decoded after GC with public operations
disabled. The earlier94-control run, original missing-char failure, compiler typo
and source-Float diagnostic mismatch remain retained separately.

Core char wraps checked Object/Double/Float cast modes16/17/18. Source kinds
are selected before storage inference; Object conversion retains exact low
integer bits for BigInt/BigInteger/Ratio/Decimal. Primitive floating calls check
bounds/NaN before truncation. Float diagnostics retain binary32 shortest digits
through the existing allocator-free, separately licensed numeric helper. Its
pinned rebuild is byte-identical. No Java dependency, type-layout change, altered
source budget, relaxed count/skip guard or size threshold is introduced.

Three String/Character distinctions and one first-class float-as-Double range
message follow the accepted UTF-16 representation and ADR-0003. They are kept
as actual JVM differences, never counted primary matches. Source provenance,
original core form/EPL notice, pinned JAR hashes and bit-pattern controls are
retained with the compatibility record.

The32 extra BigInteger/large-Ratio/large-Decimal/context controls pass in
Runtime, Macro and actual CLI AOT (`after-exact-number-carriers`,237.363s,
inputs unchanged). Combined with the earlier matrix this is401 controls per
route:397 primary matches and four explicit platform differences.
`full-suite-after-character-coercion` captures all nine original ordered char
assertions as independent host matches, unchanged source/primary/count rules.
The char-body auditor rejects the real pre-repair capture with missing
observations. The complete native suite still terminates101: broader reader
failures and assertion-head transport differences are retained in its raw capture.
The char test file alone factors existing helpers and adds those extra32 cases
after the103-test snapshot; recorded producer and prior fixture bytes match.
All15 foundation checks and Java/Node-free double bootstrap reproduction pass
with recorded inputs unchanged. Full char-source workspace/final-head CI/review
evidence remains separate.

