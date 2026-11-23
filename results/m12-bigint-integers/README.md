# Exact integer dispatch, promotion and reading receipts (#338)

These original Suss implementations follow fresh JVM Clojure1.12.6 observations.
The primary JAR/source hashes and every oracle command are retained. Clojure's
core functions, BigInt.hasheq and BigInteger.hashCode are behavior references;
no Java implementation body is shipped. Imported core forms retain their EPL1.0
provenance and explicit patches. The original magnitude kernels use GC arrays.

ABI9 retains BigInteger's plain printing through an immutable fourth integer
field. BigInt and BigInteger share the exact integer category, sign/magnitude
storage, numeric equality and hashes. Macro import/decoding and emitted literals
retain this distinction. Arithmetic produces BigInt; explicit biginteger
conversion prints without N and preserves an existing BigInteger object.

Integer comparison never widens through binary64. Hashing uses Clojure's
hashLong in long range and the signed magnitude hash outside it. Map keys use
immutable numeric equality; an i31 lookup meeting a stored BigInt falls back
from the native reference-only path to core. Both small arrays and larger tries
are exercised, including public =/number? replacements. Invalid native shapes
keep their existing fallback.

Promoting +', -', *', inc' and dec' keep exact overflow results. The source
inlining arities match the primary oracle: +'/ *' from two arguments, unary -',
and inc'/dec'. Saved function values and lexical shadows remain ordinary calls.
Clojure's nary-inline nests arithmetic; an early pair failure precedes later
operand evaluation. Source invocation facts remain the actual call rather than
an invented analyzer tree. inline-oracle.* records this independently.

Checked integer casts test the exact magnitude, and unchecked casts use its
low bits. bigint conversion of a double follows BigDecimal.valueOf's shortest
decimal spelling, rather than the exact binary significand. Runtime read-string
and suss.edn read N/radix/overflow integers through a validated magnitude fold;
skipped forms still validate without building magnitudes.

The original binary division kernel keeps quotient/remainder scratch private
until normalized values are published. quot/rem/mod preserve exact signs and
BigInt results. Runtime ratio syntax that reduces to an integer now produces the
same BigInt as the compiler reader, including Long/MIN_VALUE terms. The GC
kernel is a reference pending the measured numeric-helper port.

## Evidence and reproduction

cases.json contains67 fresh primary observations, including independently sized
signed division operands through128 limbs. clojure_bigint_integers executes them
in Runtime/Macro and an actual AOT component, plus renamed-core-var checks.
The previous50-case run and all reader/longs controls pass in
focused-division-fixed.log. The final source passes156 tests:37 focused,31 numeric controls,65 ABI
controls,19 map controls and4 bootstrap tests. Both bootstrap phases reproduce
byte-for-byte without Java/Node, and core-import/numeric-helper manifests pass.
The source-frozen command receipt has complete:true. Reproduce with:

    python3 results/m12-bigint-integers/run-gates.py

The driver validates target names, freezes source hashes, uses two build/test
threads in MemoryMax4G scopes, and retains each terminal command status. Its
checks include the original BigInt matrix, runtime readers, numeric/ABI/map
controls, Java-free bootstrap reproduction and source/artifact manifests. No
RUSTFLAGS override is used. An incomplete receipt is not a completed gate.

before-execution.json executes the older compiler's false result for (=1 1N),
where the primary oracle returns true. Original missing guards, stale unsupported
reader assertions, source assembly/check failures and a wrongly packaged ABI
test invocation remain. The initial37/50/59 fixtures retain earlier execution
scopes. Native map fallback and source-inline failures are not counted as passes.

prior-gates preserves separately qualified evidence:102 checks pass at2b778580
(the earlier three-field BigInt representation), and an unfiltered workspace run
at capture parent e8d0ed9f passes2307 tests with14 existing ignored tests. Those
runs do not certify this ABI9 source. CI failures from2b778580 remain in ci/;
the reviewed suite changed and its observations require review, not a blanket
baseline replacement.

Full Ratio values, generic exact division, rationalize and WIT u64 integration
remain outside this receipt. Other numeric API edges and conversion cases still
need their complete acceptance sweep. The helper crossover, full numeric-suite
acceptance and a workspace baseline for the final implementation remain required.
