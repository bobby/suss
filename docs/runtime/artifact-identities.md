# Portable artifact identities

Portable emission adds a versioned `suss.source-artifact` custom section. It
records the complete compiler-source fingerprint, runtime ABI/tool versions,
portable GC target/default profile/empty flags, and a digest of the complete Wasm
module excluding this one section. Source preparation also records phase, exact
source SHA-256, available Unicode filename, and the selected macro source graph.
The graph uses the compiled host's immutable loaded source/declaration ledger;
file edits do not silently replace loaded versions. Incomplete module loads and
external hosts without provenance record an unknown graph, never an empty graph.
Bare IR emission explicitly leaves source, phase and graph unknown.

The verifier rejects missing/duplicate/malformed records, incompatible compiler
builds even with the same package version, ABI/profile/flag mismatches, body
integrity changes and mismatched expected source/phase/macro versions. JSON is
bounded to 1 MiB. Unknown provenance cannot satisfy an explicitly requested source
or dependency identity. Metadata does not replace Wasm validation and linking.

Native Session installation verifies every artifact's compiler identity and
phase before allocating cells, publishing catalogs or evaluating initializers.
Bootstrap restoration additionally checks its exact source/core-import graph.
Source cache hits verify their compiler identity as well as byte integrity and
ABI. Identity annotation preserves every Wasm body byte outside its own section;
no GC layout or language evaluation order changes. Existing ABI version2 stays
unchanged, but old untagged portable artifacts must be rebuilt.

Regression-first evidence: the actual native loader accepted a same-package
artifact with an incompatible compiler fingerprint. After the guard, it rejects
before cell/catalog/residency changes. The strengthened test places a valid
artifact first, asserts the new var remains absent, and independently reads the
old value17. All six session lifecycle guard tests pass. Three compiler tests
cover identities, unknowns and malformed/corrupt records. Two native tests inspect
actual compiled macro version records, reject an older expected graph with an
unchanged expansion, and execute the program; external host provenance remains
unknown while its expansion effects execute. Six existing source cache tests pass.

Affected command/bootstrap/namespace/phase/session validation passed54 tests.
Both refreshed phase Wasm/JSON assets reproduce byte-for-byte without Java;
four bootstrap tests and Python118 pass. Independent review found no significant
scoped production defect. Its additional compiler regression preserves exact
code and custom-section bytes before and after the identity, checks deterministic
reannotation, and rejects an appended section against the old body digest.
Four compiler tests pass. Seven native lifecycle tests pass, including an actual
valid-digest wrong-phase batch rejection before any staged publication, preservation
of the old value17, and subsequent loader recovery23. Required final-head full
baseline and CI are still pending. Complete published user dependency-loader policy, component/AOT
migration, temporary evaluator retirement and scheduler/lifecycle acceptance
remain original M3 work. A manifest check alone is not their acceptance proof.
