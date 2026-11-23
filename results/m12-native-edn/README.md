# Exact native EDN data and analyzer round trips

Native tooling EDN now retains BigInt and Ratio values without a binary64
conversion or an exact-number transport tag. The compiler reader's existing
checked numeric tokenizer supplies normalized values; source/data conversion
preserves their categories. Decimal still uses its explicit unsupported boundary
and lossless source tag. Existing source conversion still recognizes validated legacy
exact-number tags. No new dependency, Java runtime or copied upstream body is added.

The native Value model keeps categories as structural tooling data, as its
existing Long/Double model does. Runtime (= 1 1N) remains true. Reader duplicate
identity coalesces Long/BigInt keys, including nested collection keys. The printer
rejects programmatically constructed maps/sets with those unreadable duplicate
keys, and integral Ratio values that cannot round-trip as Ratio. This guards the
canonical data format without silently dropping entries.

Serde integer serialization retains all signed/unsigned128-bit values, using
Long when possible and BigInt otherwise. Typed deserialization checks ranges;
integers exceeding128 bits and ratios fail explicitly where Serde lacks an
exact representation. Native EDN itself retains arbitrarily large values.

The17 fresh JVM Clojure1.12.6 samples cover N/radix/oversized integers, reduced
ratios, nested values and discarded maps/sets with distinct exact keys. Five
fresh duplicate-key probes include Long/BigInt aliases inside nested keys.
The complete reader gate passes78 tests, including these new regressions,
128-bit round trips, malformed data and both canonical printers. The original
missing exact values, incorrect discarded-key placeholder, mixed-key printer
and stale unsupported-ratio test failures remain in their original receipts.

Exact numeric literals now produce the same const-node shape as Long/Double,
with bigint/ratio type labels and exact val data. Native numeric class references
produce class constants with dotted names, rather than nonexistent vars such as
java.lang/Long. The new analyzer probes serialize/read their documents, emit
source and reanalyze it; all11 analyzer checks pass. The original missing val
and unresolved emitted-class failures are retained. The broader29 compiler
EDN/decoder/hook checks also pass on the final analyzer source, along with
Java/Node-free bootstrap reproduction, both manifests and compilation of every
workspace test target. Earlier complete-gates receipts retain the full numeric,
reader and bootstrap checks before the final class-AST refinement; input hashes
qualify each run.

The expanded25-case numeric class matrix additionally checks unequal class
values, all eight distinct class keys and lookup by an actual value's class.
Those25 cases, five protocol cases and24 raw/boxed exact-operand cases execute
in Runtime/Macro/actual AOT; eight current-var extension/GC steps execute in
Runtime/Macro. `results/m12-numeric-classes/after-native-edn.*` is terminal0 with
all recorded source and fixture hashes unchanged.

Run run-complete.py for the broad native EDN gates, run-class-ast-gates.py for
the final analyzer refinement, and run-suite.py for the unchanged vendored
corpus. Only terminal receipts establish results. Captured protocol-function
behavior after extension, complete namespace/import/Java interop and general
numeric-tower acceptance are separate limitations; this evidence does not
claim them. No primary suite reference or skip is changed by the EDN work.


The final unchanged vendored capture contains4247 raw assertion rows and is
terminal0 against the existing reviewed baseline:3739 host/61 guest/459 failed/
2680 not executed/90 failed namespaces. The audit finds zero changed assertions
and zero changed namespace diagnostics. No baseline rewrite is needed. The
final class-AST gates and full capture use identical source inputs. Only the
compiler analyzer and its new regression differ from the earlier broad
complete-gates run; final-input-diff.json retains that qualification.


The acceptance audit retains31 non-Decimal exact-number forms under whole-test
count guards; none is relabeled as an original suite pass. scoped-cast-controls.py
runs each original assertion freshly through the vendored JVM assertion reporter.
Thirty applicable forms then match in isolated Runtime/Macro/actual AOT controls
(90 observations), including checked overflow and the Double class. The one
remaining Float class assertion has no Suss equivalent under ADR-0003 Types;
its original JVM pass and explicit platform limitation remain in the source
audit. All original count failures stay in the full baseline, which is unchanged.
The required numerator/denominator/rationalize/bigint/biginteger/ratio?/rational?/
integer? calls have no remaining non-Decimal failure rows in this audited
reference. This supplements the suite evidence rather than inferring partial
success from a count mismatch.
