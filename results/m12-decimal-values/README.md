# Exact Decimal value and formatter probes

These are representation/conversion observations, not complete Decimal semantics.
The local source appends an immutable Decimal `{BigInt unscaled, i32 scale}` to
the shared recursive group as ABI11. Compiler emission, direct compiler-data
construction and macro-data encoding/decoding carry those fields. The numeric
helper adds an original allocator-free scale-preserving formatter; its GC bridge
checks scratch capacity, copies digits into private scratch and returns owned
UTF-16 storage. Decimal-to-double parses that exact text once, avoiding an
unscaled-to-double rounding intermediate. No shipped Java dependency is added.

`python3 results/m12-decimal-values/oracle.py` records159 fresh primary JVM
Clojure1.12.6/Java BigDecimal pairs: exact text and binary64 bits, with signed
unscaled integers through1024 digits and scales through both i32 endpoints.
The source, original stdout, command and JAR hashes remain here. The separately
retained forty-case decimal oracle covers the broader arithmetic/precision
contract; those expectations are not reported as passing Suss observations.

`python3 results/m12-decimal-values/run.py` passes4 tests, zero failed/ignored,
in110.14s, including compilation. The actual execution takes14.39s. The raw
no_std artifact matches every expected format, including null-output sizing,
unchanged input and poisoned scratch boundaries. Actual compiled literals link
to the shared runtime, survive GC, retain exact unscaled fields/scales and match
all159 text/Double observations. The other two tests retain the independent
decoder's adversarial owner/storage checks. `command.json` records every source
and fixture hash, actual terminal result and unchanged-input verification.

The identical new regression target and fixture are copied to frozen parent
f9d5f91a with only the existing independent integer decoder made public for the
test. `before-f9` retains its terminal101, two passes/two failures. The formatter
export is absent before; the raw literal fragment cannot link because its old
unsupported lowering imports js/Error. That second failure is a link failure,
not an observed language diagnostic. Initial placement, borrow/dependency and
fragment-signature errors remain explicit in the other original logs/receipts.

The pinned1.98.0 helper rebuild/manifest check and core-import verification both
terminate0, recorded in their logs. These scoped checks do not establish public
Runtime/Macro/AOT behavior, byte-reproduced bootstrap or a full workspace pass.

Numeric dispatch still needs exact Decimal arithmetic, equality/hash/order,
casts, reader/bigdec integration, scale-sensitive case handling and dynamic
MathContext with every rounding mode. Extending number-test/unbox alone does
not implement those operations: existing fallback paths can narrow Decimal to
Double, and the old case-literal boundary must be replaced. This local prototype
is not published as usable Decimal arithmetic. Complete original issue criteria,
suite evidence and final-head validation remain required on GitHub.

The rebased data/conversion source b6d4beb1 also passes all4 scoped tests,
zero failed/ignored, in104.52s with frozen input hashes unchanged. It includes
#499's4cf2c6d4 printer/operand fixes. Pre-rebase receipts are preserved separately.
This does not expand the verified semantic scope described above.
