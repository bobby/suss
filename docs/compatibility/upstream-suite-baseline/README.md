# PR231 exact upstream suite failure-map update at the integrated head

The suite comparator prints every changed namespace/test/assertion failure
identity and separately any changed skip map. A local focused
`cargo test -p suss-compile --locked --test clojure_test_suite --
--test-threads=1` at the integrated working head recorded exactly four
differences against the retained original baseline
(`evidence/pr231-integrated-head-suite.log`). `scripts/suite_baseline_log.py`
validates each old value, rejects duplicate, stale or unknown identities and
reconstructs every assertion's classification; its counters must exactly
match the terminal summary. This is executed comparator evidence from the
actual integrated head, **not reconstructed raw Suss values**: raw operands
and thrown payloads exist only in the run's ephemeral observations file.

All four changes are advances. `clojure.core-test.fnil` and
`clojure.core-test.get` now load, so their namespace entries leave the map.
`clojure.core-test.group-by`'s missing dependency advances from `group-by`
itself to `range`. `clojure.core-test.with-out-str`'s advances from `str` to
`with-out-str` itself. The changed map preserves all 233 remaining namespace
failures and all 20 skip mismatches, and the assertions map stays empty
because the suite's two string-get assertions (`test-get#13`, `#28`) now
pass with the genuine `String.charAt` member implemented; the prior repair
lane had recorded them as `kind:error`. Totals: 150 pass, 20 fail (all oracle
skip mismatches), 5,684 not executed, 233 failed namespaces, zero
guest-judged and zero matching skips. None of the 5,684 blocked assertions
are credited as passes.

The observed head is PR231's integrated working tree (local `dd3a9ea` plus
the staged/unstaged integration changes: str/StringBuffer retention, the
String.charAt member, the reify lowering, record iteration, the sequence
nil-guard immunity and the regenerated bootstrap). No failed run is a pass
and no unsupported result was converted. Validation:
`python3 -B -m unittest discover -s scripts -p test_suite_baseline_log.py`
(3 pass); the baseline comparison itself
(`clojure_test_suite_matches_reviewed_baseline`) is the acceptance gate for
this delta. Suite pin `95d4a9112cfc5fe6629be3d14bb99080ea00af2f` and
ClojureScript pin `c4295f303100bbf5afac449242d30bca1126f1a1`, the original
reference, vendored upstream/license and all skip expectations remain
unchanged.

The full locked workspace baseline and final-head CI remain required. This
delta does not make the 233 failing namespaces pass and does not complete
#18/#19: the five shared-collection gates, all 397 required operation rows,
complete sorted/record integration and full source/printer/Error/ES6
dependencies remain required.