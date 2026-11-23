# Scalar cast platform profile evidence

The vendored and primary JVM suites remain unmodified. MPL-2.0 patch0007 is
applied only to generated Suss copies, through the existing source-patch list.
ADR-0003 says byte/short/int results use Long and float rounds to binary32 before
returning Double. The twelve new class assertions test actual Long/Double
representations, without inventing Byte/Short/Integer/Float aliases. Four short
range-error assertions are copied unchanged from the JVM branch. All original
value assertions and clj/other-platform branches are retained.

The focused six-body regression fails before the patch:124 count failures,
28 missing short assertions, one failed namespace. It passes afterward with152
complete IDs,140 host comparisons and12 explicit guest judgments for the
primary's opaque Class report. Those12 actual Suss results additionally decode
as Boolean true; no opaque value is silently declared equal. The initial focused
test's all-host assumption is preserved as a failed harness attempt.

The complete capture executes4624 rows and records4466/75/84/2314/83 outcomes.
The audit proves exactly124 count failures and the short namespace failure
resolve, no previously passing assertion regresses, primary/vendored source is
unchanged, and all other Python AST decisions (including comparison, count and
skip guards) are unchanged except one precise patch-list append. The capture/
namespace loader prefix is unchanged; only the focused regression is appended.
The faithful baseline removes only proven obsolete failure records. Remaining
26 Decimal source-form candidates and six unexecuted Decimal-operand assertions
remain visible.36 suite comparator/lock unit tests pass. This is not a full
workspace, final-head CI or milestone acceptance claim.
