Persistent-map prototype-holder allocation probe, 2026-11-20.

The native helper already preserves each copied map/node's exact prototype
snapshot. The holder array is private and read-only for nominal objects; source
operations can obtain its contents but cannot obtain or write the holder itself.
Prototype replacement operates on native-object backing of a different descriptor,
which this map helper never admits. Sharing the holder preserves the same live
prototype object and the same per-object snapshot when constructors change later.
All physical OBJECTfield2 consumers in compiler/runtime were inspected. No shared
ABI types, runtime globals, registrations or source-core shapes change.

Require a one-element holder before admission; unsupported shapes decline before
writes to the existing source fallback. Persistent field/storage copies, identity
UIDs and metadata/hash behavior remain unchanged. This removes one allocation per
copied map/node. Internal container identity is not a Clojure behavior oracle.

The exact measured dirty patch is measured-source.patch, based on e1e46341.
candidate.json identifies the executable/core/source hashes; all seven unchanged
bench.py expressions are measured with warm-up first and median-three
instructions:u at 1000 less 0. Word-frequencies: 3,412,017.489 ->3,205,015.658,
6.066845544% fewer instructions/run. Other program counts are retained. This does
not establish the unchanged quiet5× JVM target.

The initial build was interrupted, with missing process handle, inactive scope
and no compiler process subsequently confirmed; its incomplete log is retained.
The resumed build passes10 focused tests (includes two existing decoder adversarial
tests). A later additional public snapshot/GC/closure regression brings these four
targets to11 passes, followed by byte-exact Java-free release bootstrap reproduction
and four bootstrap execution tests. The executed JVM 1.12.6 forms/results are retained.
All361 Python tests and CI foundation/source/provenance checks pass. Only tests and
evidence/docs were added after the measured runtime snapshot.

Build/probe commands use cargo +stable, SUSS_FUNCTION_NAMES=1 at build/compile,
two Cargo jobs/two test threads, MemoryMax4G user scopes, and probe-backed TMPDIR.
Run program_instructions.py SOURCE FROZEN_EXECUTABLE FRESH_TARGET_PROBE_DIRECTORY.
The frozen executable, original raw data and cycles/native-disassembly profile
remain under target/probe. Both rejected fixed-array variants and their failures
are retained separately; neither variant is included here.

Final-head correctness and quiet performance CI must establish acceptance; prior
valid failed measurements remain on issue446. No benchmark expression, divisor,
5× threshold, allocation or regression gate is changed.
