# Retained primary collection observations

These are completed Suss host captures against the unchanged pinned Jank suite
and ClojureScript oracle. They preserve unsupported, unexecuted and failed cases.
The suite source remains in `vendor/clojure-test-suite` under its retained MPL-2.0
license. `manifest.json` binds the capture, oracle and runtime artifact identities.

The array-dependency capture compares as 839 host-decided passes, 11 guest-judged
cases, 21 failures, 4983 unexecuted cases and 192 namespace failures. All 714
previously successful/guest-judged records remain identical. The 21 failures
include the executed partial/inc arity mismatch and 20 skip-mismatch occurrences
across 19 entries. These are explicit gaps, not collection acceptance successes.

Recompute the exact classification against the existing reviewed baseline:

```sh
python3 scripts/clojure_test_suite.py compare --suite suite --observations docs/compatibility/transducers/evidence/jank-array-observations.json
```

For a fresh native capture, run the focused `clojure_test_suite` binary with
`SUSS_CLOJURE_TEST_SUITE_CAPTURE` pointing to a new evidence directory. Inspect
the terminal process result before consuming captures. A failure-baseline match
preserves expected gaps; it does not certify compatibility. Each assertion is formatted on one line for review; parsed capture content is
unchanged. The previous capture
allows per-assertion regression auditing, including raw successful observations,
which the failure-only baseline cannot supply.

`validation-24805.json` records a separate completed Jank capture on implementation commit 721ad13 with the current bootstrap identity. Its parsed observations are identical to the retained canonical capture; the historical manifest remains unchanged. The Jank harness passed 9/9 and preserved all 850 prior successful/guest observations. The containing workspace run exited 101 on a stale lazy-seq support expectation (1086 passed, one failed, 22 ignored); it is not a passing baseline.
