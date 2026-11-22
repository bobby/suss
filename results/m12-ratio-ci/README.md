# Ratio integration repairs and primary review

Original CI logs at84fece33 are retained in the sibling suite receipts. Fresh
JVM1.12.6 execution confirms the corrected expectations: reciprocal2 is1/2,
a saved division function's reciprocal4 is1/4, and an unhinted nested function
receives7/2 from integer division. The adapted legacy corpus retains all201
case identities and every source expression. Only core/div-one-arg's expected
value changes from a double to Ratio, with its catalog reviewed explicitly.
The source comparator now reads the exact numerator/denominator independently.

Both legacy conformance targets pass:16 tests, with two preexisting ignored
manual recording commands. All74 reader tests and42 CLI controls pass. The
latter cover WIT parameter hints, minimal persistent sessions and two strict
reduction observation helpers; exact values are explicitly serialized instead
of silently converted to binary64. The first persistent-session fixture tried
to capture core constructors absent from a minimal runtime. Its failure remains;
the corrected fixture anchors only numeric types and keeps the runtime minimal.

An unnecessary diagnostic cleanup was reverted. The native Rust canonical EDN
model still has an explicit unsupported exact-value boundary; no invented value
or success replaces that gap. This is distinct from executed Suss read-string
and suss.edn, which accept BigInt and Ratio. The runtime numeric operations are
unchanged by this diagnostic restoration. Earlier187-test source-frozen results
are qualified separately, rather than claimed for these later repairs.

The final Java-free bootstrap reproduces both phases byte-for-byte and passes
all4 execution checks; import/helper manifests, legacy overlap and suite review
checks pass. Final-head CI remains an independent gate. The
full workspace command, complete API/tooling acceptance, WIT u64, production
numeric-helper crossover and Decimal work remain required. No issue or
milestone completion is inferred from these scoped repairs.
