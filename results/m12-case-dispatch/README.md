# Case dispatch dependency probes

`results/m12-case-dispatch/run.py before-case-dispatch` retains the terminal
93-control failure:41 mismatches in each phase, plus failed actual AOT compilation,
with unchanged inputs. The compiler still has the original scalar-only case
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
fixture, existing case expectation or failure baseline is changed. The full case
regressions remain intentionally failing until real dispatch is implemented.
