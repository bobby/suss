# Independent promotion-review fixes

The independent review of8a6e350c found Ratio unchecked-negate losing exactness
and BigInteger macro constants failing analyzer/source reconstruction. Its
separate142-expression matrix matched fresh JVM1.12.6; original commands/source
and raw results remain in independent-numeric-matrix.json.gz.

Before-unchecked records genuine Runtime and actual AOT failures:1/3 became a
double. Ratio now dispatches to its exact negate kernel. The expanded33-case
primary matrix covers direct/generic/saved-function calls, exact classes, other
unchecked arithmetic, long wrap, BigInt and signed double zero. It also exposed
positive BigInteger abs retaining its projection where Clojure returns BigInt.
The nonnegative branch now uses integer-bigint, preserving existing BigInt
identity. All33 cases execute in Runtime/Macro/actual AOT (99 observations).

Before-macro-biginteger retains the unreadable exact-number tag emitted for a
small macro constant. Wide constants previously became BigInt. Native tooling
now has a distinct BigInteger value and canonical #suss/biginteger decimal-string
carrier. Compiler selection normalizes it to Exact(Integer big:false); ordinary
runtime printing remains plain, as in Clojure. Native EDN and Runtime/Macro
readers recognize it, with configured readers taking precedence. Payloads parse
directly in base10, so010 means10. The accepted design's queue-only statement
is narrowly extended; unknown-tag failures remain. The reader helper is original
Suss code, with existing licenses retained and its source manifest regenerated.

The23 primary carrier controls cover signed/zero/leading-zero/wide payloads,
both shipped readers, source literals, lexical function shadowing and reader
overrides. They execute in Runtime/Macro/actual AOT (69 observations); malformed
cases are separately checked in both session phases. Native tests cover
source/data/Serde round trips and nested Long/BigInt/BigInteger duplicate keys.
Macro-produced small/wide/negative constants and quoted numerator/denominator
children round-trip through analyzer source. A further test executes the emitted
source as an AOT component, checking full values and BigInteger membership.

The first combined gate caught an incorrect intrinsic spelling; its failure
remains in after-both-fixes. The corrected gate caught the positive abs mismatch,
retained in after-intrinsic-fix. The final focused gate passes16 tests with
source/fixtures unchanged. The broader run-gates.py is terminal0 with80 reader,
50 CLI,88 compiler/ABI/decoder and4 bootstrap tests (222 total), Java/Node-free
double bootstrap reproduction, both manifests and compilation of every workspace
test target. The emitted-source AOT check is a separate passing test added
afterward; production source is unchanged.

The final unchanged vendored capture is terminal0 with4247 raw rows and the
existing3739 host/61 guest/459 failures/2680 unexecuted/90 failed namespaces.
The audit finds zero changed assertions or namespace diagnostics. No primary,
skip, count guard or baseline rewrite accompanies these fixes. Historical
measurements and unfiltered workspace results retain their original source
qualification. Final-head CI and independent delta review are separate gates.
