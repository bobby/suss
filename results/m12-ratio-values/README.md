# Exact Ratio values and integer division (#338)

These original Suss implementations follow fresh JVM Clojure1.12.6 observations.
No JVM dependency or Java implementation body is used in shipped code. Primary
JAR/source hashes, full commands, expected observations and diagnostic adapters
are retained beside this file. RT/Ratio bytecode inspections are development
behavior references; imported core forms retain their existing EPL provenance.

ABI10 appends an immutable Ratio with two integer children. The denominator is
positive and greater than one; normalization reduces by GCD, and an integral
result becomes BigInt. Exact division of ordinary longs can retain Long; MIN
operands and BigInt operands preserve Clojure's arbitrary-precision route.
Addition, subtraction, multiplication, division, quot/rem and comparison operate
on exact fractions. Double operands select floating arithmetic. BigInt abs
also retains exact values. Integer/Ratio case constants use numeric-category
matching and canonical duplicate keys, with selector evaluation preserved.

The original GC reference division/GCD kernels own their scratch and publish
only immutable results. Compiler literals have already been normalized by the
reader and use a private constructor without repeating GCD. Runtime readers and
arithmetic still normalize. Numerator/denominator are stable BigInteger fields;
BigInt conversions truncate exactly. Checked casts narrow the exact quotient;
unchecked int/byte/short use Ratio.intValue's saturating double-to-int route.
The diagnostic cases retain the exact primary messages.

Ratio-to-double first rounds to16 decimal significant digits, HALF_EVEN, then
uses the pinned binary64 parser. It does not divide independently rounded double
magnitudes. Ties, overflow, signed underflow and wide numerator/denominator shapes
are exercised. rationalize uses the double's shortest decimal spelling. Generic
source aliases, hints and saved function values are included in the matrix.

## Observations

The104-case fresh primary matrix passes in Runtime, Macro and actual AOT (312
matrix observations), plus compiled-macro arguments/results and retained-child
GC checks in both phases. The initial73-case AOT observation and failed session
fuel probes remain separate. The large eight-operation1024-bit rational stress
case has an explicit10-billion test fuel budget; production limits are unchanged.
GC reference-kernel cost remains a limitation pending the numeric-helper bridge.

Reproduce expectations with `python3 results/m12-ratio-values/edge-oracle.py`
and `python3 results/m12-ratio-values/oracle.py`. The edge oracle includes case,
identity, hints and diagnostic messages. The combined oracle records every final
expectation freshly. Its field-access adapter uses JVM getMessage for Suss's
owned message field. The original missing adapter, unsupported-case fallback,
checked cast error, missing namespace fixture and redundant-normalization fuel
failures remain in their original logs/recordings.

The focused Cargo command includes both previous BigInt matrices,
long/double controls and runtime readers. All41 tests pass in its terminal log, with hashes in focused-command.json.
After integrating the published analyzer/observation fixes, run
`python3 results/m12-ratio-values/run-gates.py` for source-frozen nearby numeric,
ABI, map, analyzer/decoder, bootstrap and manifest checks. The source-frozen driver passes187 Rust tests and35 Python checks, reproduces
both bootstrap phases byte-for-byte without Java/Node, and validates both
manifests. Its inputs retain the exact tested source. It refuses to run without
the published source fixes in its ancestry. Subsequent CI expectation repairs
and restoration of the native tooling diagnostic are qualified separately.

Full numeric-suite acceptance, WIT u64 above the long range, the production
helper crossover and a complete workspace baseline remain required. BigDecimal
and with-precision remain separate numeric-tower work. No issue or milestone
completion is inferred from this matrix.

The frozen pre-Ratio executable returns0.3333333333333333 for integer `/` and
fails its Ratio-literal export. before-execution.json retains the actual
executable/source/component hashes, commands and results. The initial fixture
assumed the old CLI exposed its language error text; its failed assertion is
retained. The final check asserts the actual failed call without inventing the
missing diagnostic.

The native Rust canonical-tooling EDN model still lacks exact numeric values;
its explicit boundary/diagnostic is retained. This is separate from executed
Suss read-string and suss.edn, which accept BigInt and Ratio. Complete numeric
acceptance must audit this tooling representation as well.
