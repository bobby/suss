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
