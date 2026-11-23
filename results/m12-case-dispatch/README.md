# Compiled case execution evidence

`results/m12-case-dispatch/run-regression-gates.py
regression-after-shared-provenance` passes all six groups:13 CLI case tests,
independent decoder/privacy checks, all13 original unboxed-local guards,
call-site and AOT size limits, Java/Node-free bootstrap reproduction/execution
and workspace all-test compilation. All recorded inputs remain unchanged.
`run-foundation.py foundation-after-shared-provenance` passes all15 checks,
including364 Python tests. `preserve-baseline-layout.py` then preserves the
original JSON layout: exactly four case-namespace lines removed and no other
byte changes from the prior baseline. The full workspace run, final-head CI
and independent promotion review remain separate required evidence.

`run-case-gates.py compiled-case-gates-after-message-borrow-fix` passes the full
72-assertion vendored case body, independent retained-field/GC checks in both
phases, duplicate diagnostics and compiler-owned origin/privacy controls.
`regression-gates` additionally passes all13 CLI case tests before exposing the
original AOT size failure (3985 >3900). No size guard was relaxed.

The CLI tests execute328 distinct JVM-confirmed controls in Runtime, Macro and
actual AOT:93 dispatch,203 private hash,26 collision and six str controls.
Ordinary sessions use the ordinary JVM reference; AOT uses pinned JVM direct
linking. Exactly one original default-message expectation differs between those
reference modes. The initial AOT `replacementreplacement` result is retained;
fixed direct core str conversions now preserve the JVM-confirmed behavior.
The collision matrix includes rare equivalent Decimals with equal Java hashes
across scales, as well as nested maps/sets/vectors and observable public equality.

`run-suite.py suite-after-manifest-refresh` records5002 raw assertions with
source inputs unchanged. The original Rust exit101 reports baseline drift;
`audit-suite.py` proves all4860 prior individually comparable passes remain,
with exactly72 new host matches. `write-reviewed-case-baseline.py` removes only
the resolved case namespace record. All assertion/test failures and count/skip
rules remain unchanged:4842 host matches,75 guest judgments,86 explicit failures,
1936 unexecuted assertions and72 failed namespaces. The separate host comparison
passes; the original Rust result is not relabeled as a pass.

The first suite scope was interrupted only after a proven stale review-overlay
manifest, with the interruption and source hashes retained. The overlay hash was
refreshed; core source bytes did not change. The subsequent complete capture has
its own terminal receipt. Numeric source candidates decrease15 ->11; the actual
Decimal-operand subset remains194 matches and six unexecuted eval/EDN assertions.
These results do not establish complete BigDecimal acceptance.

`run-provenance-gates.py actual-component-provenance-after-export-symbol` passes
all provenance unit/integration and unchanged size checks. Standalone identities
remain complete format2; the assembled component shares entire macro graphs in
its input-provenance section. The actual component executes successfully, retains
all13 fields per original input and both macro versions. The independent pinned
JVM reader (`provenance-independent-reader.clj`) compares the real component
section to all five raw original EDN sections. Earlier test compilation and export
symbol construction failures remain in their own directories. Current source
formatting and broader regressions have separately qualified later receipts.

The schema, limits and optimizer qualification are described in
`docs/runtime/compiled-case.md`. No unfiltered workspace, final-head CI or
independent promotion review is claimed by these focused receipts.

## Earlier private hash checkpoint


`results/m12-case-dispatch/run.py before-case-dispatch` retains the terminal
93-control failure:41 mismatches in each phase, plus failed actual AOT compilation,
with unchanged inputs. At that checkpoint the compiler retained its scalar-only case
bootstrap. No case assertion is relabeled as passing by the hash work below.

`oracle-numeric-hash.py` records114 fresh pinned JVM Util.hash controls;
`before-numeric-case-hash` retains their unresolved-helper failures.
`after-numeric-case-hash` passes both phases. An original native adapter keeps
Java hashCode distinct from Clojure hasheq: Long and BigInt long-part hashes,
BigInteger magnitude-word hashes, Ratio xor, Decimal coefficient/scale and
canonical-NaN Double bits with signed zero preserved. It reads no MathContext.
`oracle-composite-hash.py` adds89 primary identifier/collection controls.
Original compiled Suss helpers handle UTF-16 strings, symbols, keywords,
sequences, maps and sets without a mutable hash cache or public hash call.
Provenance is in `docs/compatibility/bootstrap/clojure-case-hash.json`.

`run.py after-composite-case-hash hash_` passes all203 controls in Runtime,
Macro and actual CLI AOT, source/fixtures unchanged. A separate frozen executable
records the missing collection helper before its installation; this one probe
is not presented as a complete89-control before run. The29 additional fresh
collision/duplicate boundaries retain actual JVM callback traces and distinguish
Decimal duplicate equality from matching, signed-zero duplicates and accepted
repeated NaN constants. These reference probes are not Suss dispatch passes.

`run-hash-gates.py hash-gates-after-test-type-fix` passes32 tests and the import
manifest check, with unchanged inputs. Eleven retained-value controls independently
decode canonical storage after GC in both phases, including Decimal scale and
Double signed-zero bits, while public predicates/hash/equality/printer are
redefined to fail. Original13 unboxed guards and17 import tests pass unchanged.
The AOT limits remain3895 <=3900 bytes/form and96 <=128 extra split-form bytes.
The initial independent-test compile error is retained separately; only the
new test's attempted Clone use was fixed, with decoder semantics unchanged.
`run-bootstrap.py bootstrap-gates` passes the unchanged Java/Node-free double
reproduction/identity script and all four compiled bootstrap execution tests.

This is private hash/equivalence infrastructure. Literal dispatch, quotation,
duplicate diagnostics, omitted defaults and complete72-assertion case execution
still require integration. The identity fallback is not certification of arbitrary
Java-object hashCode interop. No full workspace run or final-head CI is claimed
for these new helpers; earlier Decimal full-run evidence retains its own head.


Primary Clojure/core forms, exact JAR/source hashes and EPL references are retained
in `clojure-case-primary-source.json`; runtime/compiler and numeric method
bytecode commands retain their actual arguments and output hashes. No primary
fixture, existing case expectation or failure baseline is changed. The original failing case
regressions remain recorded separately from the later real dispatch results.
