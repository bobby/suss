# Core predicate observations

The reviewed core importer adds `char?`, `coll?`, `sorted?`, `pos-int?`,
`neg-int?` and `nat-int?` from the pinned ClojureScript declarations. EPL notices,
complete extracted originals, source/patch hashes and explicit review-overlay
entries remain reproducible. Collection predicates retain their protocol bodies.
`char?` retains the accepted one-unit UTF-16 string representation. Integer sign
predicates use the canonical private Long representation, rather than the
JavaScript integer/wrapper dispatch: integral Double, BigInt/BigInteger, Ratio
and Decimal values do not satisfy these Clojure predicates.

`oracle.py` executes JVM Clojure1.12.6 with pinned JAR hashes. The273 controls
cover numeric categories and boundaries, collection kinds, UTF-16 characters,
aliases, apply, and once-only argument effects. `before-core-predicates` records
273 missing-name failures in each phase, frozen inputs unchanged.
`after-portable-character-controls` passes every control in Runtime, Macro and
actual CLI AOT, with a frozen fixture and all source inputs unchanged. Source
qualification is the0436ab30 base plus the recorded input hashes; no publication
or final-head CI pass for these ports is inferred.

The first oracle rejects a surrogate *character literal*, and its next attempt
correctly records JVM root-binding set! errors. Both failed attempts are retained.
The successful original273-control fixture's one surrogate control calls public
`char`, which Suss separately lacks; the first after run records that failure in
both phases and AOT. The final fixture constructs the same character using first
on a one-unit UTF-16 string. A fresh JVM observation is still true. Exactly272
cases remain byte-identical, and the one changed case is audited in
`fixture-adaptation.json`. `before-lone-surrogate-cli.json` independently proves
the final control fails before the port, using the immutable pre-port executable
with its SHA256, actual argv, terminal status and complete output.

The initial before run started before fixture generation finished, so its receipt
omits the fixture hash. Its failures are retained as
`initial-incomplete-fixture-receipt`; the subsequent qualified before run
captures the finished fixture and supersedes it for regression evidence.

`provenance-commands.json` records four passing groups: strict review validation,
reproducible514-file import, core-import unit tests and review-overlay unit tests.
These are provenance checks, not executing semantic certification. The complete
vendored predicate bodies and unfiltered capture have separate receipts.

The complete char? body contains two JVM String judgments that differ from the
accepted platform representation: one-unit strings are also characters in Suss.
Those differences remain explicit failures, with original primary values and
count/skip/comparison rules untouched. Public char coercion, general eval,
regex/UUID readers and composite case constants are not supplied by these ports.
No full Decimal acceptance follows from this bounded evidence.

`after-complete-predicate-bodies` is a retained strict failure:150 of173 assertions
execute,148 host-match and the two documented char? judgments fail.23 sorted?
assertions remain unexecuted because sort is also missing. The regression's full
expected body is not weakened to match that partial result. The15-command
foundation run passes all364 Python tests with frozen inputs unchanged.

The adaptive sort followup uses the actual Clojure numeric comparator conversion,
object-array mutation versus primitive-array copy rules, natural runs, stable
binary insertion and balanced adjacent-run merging. `after-stable-sort` retains
the initial two per-phase mismatches; the direct Ratio/Decimal comparison is
correctly still a nonterminating-conversion error. JVM callback traces explain
why short-input run detection avoids that pair. `after-adaptive-stable-sort`
passes65 controls (44 base,7 staging and14 larger stable ordering matrices) in
both phases and actual CLI AOT. No comparator control was removed or weakened.
General Java Comparator object interop, galloping and every invalid-comparator
trace remain outside this bounded evidence.

The initial decoder test used raw decode and was correctly rejected by its
ownership guard. Its complete log is retained separately. The corrected before
run uses decode_session and reproduces unknown sorted storage in both phases.
`after-tree-collection-decoder` passes all28 existing/new tests with canonical
TreeMap/TreeSet owners, physical node fields, counts, budgets, GC and disabled
guest printing/equality. Hash-set backing-map restrictions and every existing
owner/layout guard remain. `after-sorted-storage-decoding` then passes the full
173-assertion regression:171 host matches, two explicit accepted String/char
platform differences, zero skips/unexecuted/failed namespaces. Original expected
counts and primary/comparison decisions are untouched.

All six compatibility groups pass with53 Rust tests (including102 constructor
controls) and workspace test compilation; the final15 foundation commands pass
364 Python tests. Bootstrap reproduction/execution needs no Java or Node. AOT
sizes remain3895 <=3900 bytes/form and96 <=128 extra split-form bytes. Every run
retains frozen source receipts; production CI and full-workspace acceptance of
these separate unpublished ports are not inferred from these bounded gates.

The final unfiltered capture (`suite-after-sort`) records4930 raw observations
with unchanged frozen inputs. The original test's exit101 is expected baseline
drift, not a successful Rust full-suite command. `audit-sort-suite.py` independently
compares the original primary and reviewed previous capture, preserving every
individually passing observation and all count/skip/routing/value decisions.
`write-reviewed-sort-baseline.py` checks the exact changed-ID sets before writing
and rechecking the faithful baseline:4770 host /75 guest /86 failures /
2008 unexecuted /73 failed namespaces.304 additional host matches and ten newly
loaded namespaces are established. Five prior undecodable rows resolve; seven
newly executed failures remain (four assoc arity cases, two char platform cases,
one unknown record). No previous-pass regression occurs. Sort-by's namespace
still fails, now at its own missing function.

Fifteen Decimal-related source candidates and six of200 actual Decimal-operand
assertions remain failed/unexecuted. The latter subset has194 matches. Neither
subset establishes complete BigDecimal acceptance. The exploratory22 JVM case
boundary probes retain distinct matching/hash-collision/duplicate rules for the
next implementation; they are primary observations, not passing Suss evidence.
The published PR500 head remains separate and unchanged while its full baseline
and promotion review gates run.
