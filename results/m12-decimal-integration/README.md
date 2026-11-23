# Exact Decimal arithmetic and subtraction observations

This isolated integration starts at f36cc934, preserving the already published
reader/class/ABI repair and the original arithmetic worktree. The native ABI12
MathContext is immutable precision/mode storage, and its per-phase getter reads
the ordinary dynamic Var. Arithmetic retains exact BigInt coefficients and
checked signed32-bit scales. Finite-precision addition bounds distant alignment
with a signed sticky digit below the coefficient and rounding positions. Native
comparison orders signs/adjusted exponents before bounded alignment; hashing
strips trailing zeros using the BigInteger word hash without changing values.
Bigdec parses exact decimal strings and canonical Double text; Ratio conversion
uses the current context. Adapted with-precision source retains EPL provenance.

Source numeric overload facts select direct primitive subtraction independently
of raw storage. Boxed checked/promoting/unchecked subtraction first negates in
the right category, then adds. Focused regressions preserve Decimal rounding
stages, Ratio sign/conversion order, MIN overflow/promotion/wrapping, signed zero,
source evaluation order, captures, loop reanalysis, hinted methods and macro
transport. Existing unboxed, capture, box reuse and primitive-method tests pass.

The observer anchors canonical numeric owner types before evaluating fixtures.
Decimal observations expose the actual BigInt coefficient and scale after GC,
without guest printing/equality or host normalization. Mutable owner impostors,
malformed coefficient metadata and exhausted decode budgets fail. Strict primary
operand comparison retains preferred scale and trailing coefficient zeros,
separately from user-visible Decimal equality across scales. The primary oracle,
assertion counts and skips are unchanged.

## Execution receipts

- Before-change subtraction controls and independent decoder/comparator checks
  fail as recorded. Original arithmetic, source, harness, constructor and literal
  failures remain in this and the preserved m12-decimal-arithmetic directory.
- after-subtraction-overload-dispatch passes the initial196 controls in each
  phase. after-independent-decimal-decoder also executes actual CLI AOT and89
  direct coefficient/scale controls per phase.
- run-gates.py after-macro-setup passes all five commands with frozen inputs:
  49 tests across six Decimal/ABI targets,36 tests across11 numeric regression
  targets, workspace test compilation, and both source/import manifest checks.
  Its228 behavior controls execute in Runtime, Macro and actual AOT;89 field
  controls execute after GC in each phase:862 scoped observations. These counts
  describe controls, not distinct tests or full-suite acceptance.
- The initial expanded run's session-macro setup error is retained in gates.json
  and decimal-and-abi.log. Macros are defined through CompiledMacros.define.
  Initial JVM macro-in-let controls were replaced with separately defined macros;
  the original input/output records remain under initial-subtraction-source-facts.
- All36 strict comparator unit tests pass after the new Decimal case. The exact
  before/after comparator sources and terminal statuses are recorded.
- post-gates-input-diff.json qualifies later documentation and removal of
  unrelated rustfmt whitespace; production implementation files are unchanged.
  protected-original-wip.json verifies the original arithmetic HEAD and patch.

The complete vendored capture, full workspace execution and Decimal cast
acceptance still need fresh results. decimal-casts-oracle.py records109 fresh
JVM controls; it is not a Suss acceptance result. The primary controls show that
checked float and unchecked-float may round Decimal differently, and checked
versus unchecked integer conversions follow different Clojure paths. Do not
replace these expectations with Suss observations or infer exact casts from
binary64 conversion tests. No final-head CI or independent promotion review is
claimed for this local integration.

archive-index.json lists large receipts archived losslessly as .json.gz with
uncompressed SHA256 hashes. The original local plain receipts remain intact.
JVM commands, pinned JAR/source hashes, raw stdout/stderr, Java21.0.13 version
and primary-source reference hashes remain alongside their fixtures. No Java
source body or shipped Java dependency is added.

## Complete arithmetic capture

suite-after-arithmetic/suite-commands.json records the build and unfiltered capture
at45a20341:4585 actual assertions in373.89s. The original baseline comparison
terminates101; the lock check passes. audit-suite.py retains every changed primary
and actual row, proves unchanged count/skip logic and source routing, and finds
172 resolved assertion failures, two newly loaded namespaces, no previously
passing regression, and a changed diagnostic in the still unloaded case namespace.
Counts:4322 host matches,63 guest judgments,212 failures,2342 unexecuted and84
failed namespaces. The faithful reviewed baseline write and comparison pass;
remaining semantic gaps are preserved, not relabeled as success.

The first compact audit incorrectly inferred200 matching Decimal rows from the
absence of explicit failure entries. Namespace failures are inherited. The complete
audit requires actual observations and records194 matches plus six unexecuted
rows (eval and EDN/uuid), with58 broader unresolved Decimal source candidates.
This does not establish issue acceptance. The initial mistake is retained.

The original CLI println harness cannot resolve println; it records no numeric
observations. The subsequent vector probe executes all109 cases:73 match the
original JVM text fixture. Since the accepted design returns widened Doubles for
float casts, a fresh primary fixture explicitly applies double on both sides;
75 match and34 differ. Original Float text controls and Suss output remain
untouched. The real differences cover checked range staging/messages, exact
unchecked narrowing, rationalize and unchecked binary32 rounding at a binary64
tie. These are still failures. No full workspace execution, final-head CI or
independent promotion review is claimed for the local arithmetic integration.
