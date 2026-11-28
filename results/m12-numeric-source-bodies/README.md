# Complete numeric-dependent source bodies

The first exact native command returns101 in191.22s on unchanged source/HEAD.
All142 original eq/not-eq assertions execute:139 host matches and three actual
boxed-NaN verdict failures (#70 in each body and not-eq#71). Their original
Decimal/Ratio rows12/68/69 match independently. The existing MPL patch0006 gains
only a :suss guard for the unchanged six Clojure-rule assertions already under
:clj; the original vendored bytes, original primary, protected harness/comparator,
skip/count/opaque rules and reviewed baseline remain unchanged.

The complete original default-num body cannot load: the public num var is
missing. The initial supplemental adapter additionally lacks standard reference
IDs, so its comparison raises KeyError before evaluating results; this is a test
adapter defect, not a native pass claim. The capture retains the actual namespace
failure and zero observations. Do not use subset numeric matches as complete
source-body acceptance.

The supplemental pinned JVM recording selects the complete original num source
with tools.reader1.5.0 features#{:suss} and executes it on Clojure1.12.6 using the
unchanged original primary encoder. Its actual exit is1:26 assertions,23 pass and
three Short/Byte/Integer preservation failures (ordinals7/8/9), with no reporter
errors. JVM num widens those narrow boxes; Suss deliberately has none (ADR-0003).
These three actual discrepancies must remain explicit in native comparison;
never rewrite the primary verdicts, count them as host matches, or fabricate the
JVM-only Checker. This supplemental recording never replaces, slices, normalizes
away or changes the original global JVM/clj18-row num primary.

Compressed receipts and raw captures retain the before-failures. The source
branch's full workspace, independent review and final-head CI retain separate
gates; no numeric issue closure or promotion is inferred.


## Clojure num source adaptation and selected-body execution

The entire pinned one-argument num form and original license are retained under
docs/compatibility/num. The JVM Numbers/num cast becomes Suss's checked nil/Number
category projection, returning the actual argument. ADR-0003 has no narrow boxed
Number classes to widen. Core-owned guard calls retain normal rebinding policy;
non-number inputs throw. No Java implementation, Checker or Java dependency is
shipped. Source/provenance is under docs/compatibility/bootstrap/clojure-num.json.

Four independent development JVM runs retain exactly the same26 observations:
actual exits1 each,23 pass and3 fail. A separate supplemental metadata builder
adds the ordinary canonical assertion IDs/forms/determinism fields after comparing
all four complete raw recordings. It preserves every actual verdict/value and
never changes or calls a relaxed version of the global primary normalizer.

The first source-port run returns101 because I started it before its supplemental
reference file existed (the metadata builder had a syntax error). Its native
capture nevertheless records the entire namespace loaded and26 actual passing
Suss assertions; the receipt is preserved as a failed comparison, not a pass.
After correcting setup, the exact focused body command returns0 in9.38s on
unchanged HEAD/source. Standard unchanged comparison yields23 host matches,
3 explicit Short/Byte/Integer platform differences (ordinals7/8/9), zero guest
judgments, zero skips, zero unexecuted rows and all26 ordered identities. Ratio,
BigInt and Decimal rows4/5/6 match independently. The global JVM/clj18-row num
primary remains unchanged. This selected-body result does not establish separate
Macro/AOT controls, full workspace, review or final-head CI acceptance.

The six-rule eq source adapter is now a zero-context patch hunk and retains
original assertion bodies and context labels. This removes whitespace-only
patch context from the repository diff; native source generation and the
unchanged upstream lock check return0. All142 eq/not-eq observations still have
a strict passing requirement. Their3 actual boxed-NaN failures remain unresolved;
none is skipped, reclassified, removed, or made into a successful comparison.
