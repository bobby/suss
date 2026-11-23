# Decimal reader, class and ABI integration evidence

The original #500 CI at f1c52dd0 failed four partitions. The complete original
job logs and check summary are retained here. Two ABI tests expected10 although
the Decimal layout uses11; runtime/macro reader tests still expected Decimal
values to be unsupported; the full vendored capture differed from its earlier
reviewed baseline. None of these test outcomes is treated as an arithmetic pass.

An isolated checkout replays the two published Decimal commits onto ad46b2ee,
preserving #499's class-call, printer, exact Ratio and BigInteger fixes. It adds
an exact checked private Decimal constructor, runtime reader integration and
native BigDecimal class dispatch. The constructor is registered after checked
integer casts, uses the raw i64 cast and range-checks the signed32-bit scale.
The original reader accumulates nine-digit long groups into a BigInt and parses
exponent signs explicitly. No coefficient is rounded through a Double. The
bootstrap source is original Suss code; no upstream body or Java dependency is
added. Core-import source hashes and generated manifests are regenerated.

`oracle.py` runs fresh Clojure1.12.6 with the pinned development JARs. Its169
controls include literals/printing, runtime read-string, class/Number membership,
Class-call evaluation order, retained generic operands, scalar constants and
protocol dispatch. The159 coefficient/scale controls are freshly rerun through
read-string, including signed32-bit scale endpoints and1024-digit magnitudes.
`oracle.json` preserves the exact command, JAR/source hashes, stdout and stderr;
`cases.json` is the execution fixture. The original ten-case fixture and oracle
and the initial JVM harness failure remain separately retained. The positive
2^31 exponent boundary is checked against the JVM, not inferred from a negative
exponent-overflow diagnostic.

`run-focused.py final-focused` terminates0 with28 passes, zero failed/ignored,
in27.88s, and unchanged source/fixture hashes. It executes the169 controls in
Runtime, Macro and actual CLI AOT; both shipped readers' existing corpus; actual
Decimal fields/scale/format/Double bits after GC; raw helper scratch guards; and
the unchanged call-entry/allocation/descriptor/shared-type invariants. ABI11 is
accepted and earlier versions, including10, are rejected.

The new large-coefficient session stress matrix uses100M fuel, as the printer
stress tests do. Its default10M combined parse/print exhaustion remains in
`after-raw-scale-cast` and `after-chunked-coefficient`; no existing test budget or
production limit changes. The latter moved the exhaustion beyond the original
BigInt digit fold but did not make the full combined operation fit10M. Compiler
harness import, unsupported protocol-helper, constructor registration-order and
boxed/raw-cast failures remain in their corresponding original receipts. They
are not claimed as executed semantic passes.

`protected-wip.json` verifies the separate arithmetic checkout: same HEAD,
49 identical modified/untracked file hashes and byte-identical tracked patch.
The unfinished ABI12 arithmetic/MathContext work is not part of this repair.

These probes cover representation, conversion, literals/readers, formatting and
the retained class/constant/protocol cases. Exact arithmetic, dynamic precision,
rounding, equality/hash/order/casts, bigdec and scale-sensitive case remain
incomplete. Existing numeric fallback can still narrow Decimal to Double. The
original issue acceptance criteria remain required; this evidence does not
justify promoting the Decimal draft or closing its issue.

The unmodified full-suite runner captures4517 raw assertions in312.36s and
terminates101 against the earlier reviewed baseline. `suite-audit.json` records
all243 changed assertion failure records, one resolved whole-test interruption,
and four newly loaded namespaces. It finds zero regressed previously passing
assertions. `newly-executed-rows.json` retains every primary/actual row in those
new namespaces: decimal?40 passes; mod72 passes/12 failures; quot57/12; rem56/12.

Across all namespaces, the unchanged comparator records4082 host matches,63
guest judgments,384 failures,2410 unexecuted assertions and86 failed namespaces.
The106 previously erroring Decimal predicate/Boolean/text/small-number controls
now match;84 other evaluation errors become explicit verdict failures. Seq
completes all17 assertions, with three retained failures, including an opaque
Decimal map observation. The independent decoder/comparator still cannot accept
Decimal observations, and numeric fallback remains limited. These counts do not
claim exact Decimal arithmetic/comparison or complete suite acceptance.

The guarded baseline write requires the checked-in baseline to equal the
archived old value and rejects any previously passing assertion regression.
Only then is the faithful observed failure record written. The subsequent
compare and suite-lock commands pass. The primary reference, skips, assertion
count guards, decoder and comparator remain unchanged. All original terminal
failures are retained.

`run-gates.py` completes all eight commands with terminal0 and frozen input
hashes unchanged:227 broader test executions; identical two-build Runtime/Macro
bootstrap reproduction without Java/Node; both source/import manifests; and
compilation of every workspace test target. The scoped tests overlap the
focused reader controls and are not counted as distinct semantic cases. This
is not an unfiltered local workspace pass. The final input diff explicitly
records the documentation, explanatory test-comment and faithful baseline
changes after the focused/capture commands. No executable implementation changes
follow those successful source snapshots.

Large JSON receipts, the complete raw suite capture and detailed audit rows are
archived byte-for-byte as `.json.gz`; their original uncompressed local files
are retained. Logs preserve original bytes. `suite-summary.json` gives the
compact counts and changed-record totals without replacing the detailed audit.

`unchanged-oracle-inputs.json` records verified input hashes from the reviewed
base. The published Decimal prototype makes only the host integer formatter's
visibility public for its raw-field test; the decoder body is byte-identical.
The primary reference, harness count guards and comparison script are identical
to the reviewed base. The audit verifies these input hashes before judging rows.
