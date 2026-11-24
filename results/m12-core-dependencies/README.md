`results/m12-core-dependencies/oracle.py` records396 fresh JVM Clojure1.12.6
controls for min-key and some-fn. Full upstream forms, EPL notice, source/JAR
hashes and def/fn adaptation patches are retained. Key functions execute once;
min-key uses < for the first pair and <= thereafter, preserving ties/NaN. The
some-fn controls cover1..5 predicates,0..6 arguments, every hit position,
short-circuiting, returned truthy values and exact predicate/argument traces.
`run-prototype-repl.py` executes396/396 matching rows through the actual CLI,
with each request at an explicit ordinary10M compilation allowance, executable
hash unchanged. These are local definitions, not a production-core, Macro or
AOT pass. Six pure map-indexed fixture constructors are materialized because
that helper is unported; separate fresh JVM execution verifies all396 values
identical to the untouched primary controls. No function under test changes.
Earlier prototype failures (missing println, batch analysis fuel, unavailable
map-indexed and an unsupported -e compilation option) remain terminal receipts
with zero observed controls, never passes. No production budget is changed.


The shipped import at9d501d71 has separate production evidence:
`run-min-key.py after-checked-comparison-capture` passes all32 controls in Runtime,
Macro and actual AOT, with retained host decoding and unchanged inputs (688.215s,
four tests including two adversarial decoder tests). `run-min-key-body.py` passes
all49 ordered assertions of the complete unchanged vendored body against the
pinned primary (65.349s, zero guest judgments/failures/skips, inputs unchanged).
The original missing-name before failure and the initial import's real
nonnumeric-key failures remain archived with their own source identities.
`min-key-shipped.sus`, `min-key-shipped.patch` and `min-key-shipped-adaptation.json` capture checked numeric comparison
function values lexically; all complete upstream arity bodies are retained.
The historical local-definition adaptation/prototype receipts remain untouched.
Full capture/audit, baseline, workspace, bootstrap, CI and independent review
remain separate gates; these focused results do not close the numeric issues.

The full a374b92d capture contains5,357 actual rows and preserves both previous
pass sets, with all49 min-key rows matching on the host. Its original native101
receipt records the unexpected successful namespace load against the old baseline.
The guarded baseline update removes only that load failure; unchanged host
comparison then returns0. BigInt/Ratio has226/229 host matches; Decimal199/200.
No unrelated failure or unknown row is replaced with success. Complete raw
capture/audit/baseline-write evidence is archived in
../m12-reader-followup/full-suite-after-shipped-min-key. Broader gates remain
separate from this comparison.
