# Portable numeric classes

The original Rust/Wasm implementation gives Long, Double, BigInt, BigInteger,
Ratio, Number and Class stable, non-callable descriptor values. BigDecimal has
a class value only in ABI10; this does not add Decimal instances or arithmetic.
There is no Java dependency in shipped code. `class` returns the actual value's
constructor, separately from any metadata-sensitive `type` behavior.

The JVM Clojure1.12.6 recordings cover printed names, class identity,
positive/negative membership, default imports, syntax quote, callability,
class-key map lookup, protocol dispatch and Number inheritance. The final
24 basic cases run in Runtime, Macro and an actual AOT component (72 basic
observations). Five protocol cases also run in those three routes. Eight
extension steps run in Runtime/Macro, alternating numeric categories at one
site, changing Number and specific class extensions, testing markers and
generic `apply`, with explicit GC between steps. `after-live-extension.json`
is terminal0 for the earlier23-case matrix. `final-combined.json` is terminal0
for the expanded24-case matrix and the exact-operand target, with all recorded
source and fixture hashes unchanged.

Native numeric/class receivers carry distinct protocol cache kinds. Their
dispatch checks the actual class descriptor, then Number inheritance, before
the earlier native-kind/default fallback. A shared native cache cannot bypass
that lookup. Source object and packed-array class values use Class receiver
identity; they do not become ordinary callable functions for protocol lookup.

Failures are retained: the original unresolved BigInt class, the missing public
class and ifn? adapters, class-specific protocol dispatch, and missing Number
inheritance. The original bootstrap adapters for class/ifn? are Suss code, not
copied upstream forms. Existing core import notices and licenses remain.

The captured-function experiment in `live-oracle-first.json` exposes a wider
protocol limitation: a JVM function value captured before extension keeps its
old implementations, whereas the existing Suss dispatcher consults current
tables. `class-protocol-oracle.json` isolates that behavior. The passing live
extension matrix calls the current protocol var; it does not certify captured
function parity, general JVM interop, or complete namespace/import behavior.
`live-oracle.py` retains all JVM output, including defprotocol result lines.

Run `python3 results/m12-numeric-classes/run-gate.py LABEL` for the class target,
`run-focused.py` for surrounding protocol/nominal/bootstrap gates, and
`run-suite.py` for an unchanged vendored-suite capture. Only terminal receipts
establish results. The suite's primary opaque Class observations are not host
boolean matches; the independent positive/negative JVM matrix provides that
control. The reviewed before baseline is frozen separately from new actual
observations; no new failure is silently selected away.


The full capture contains4247 actual assertion rows. Its reviewed comparison
reports3739 host matches,61 guest-judged,459 failures,2680 not executed and90
failed namespaces. BigInt contributes14 host matches and four guest judgments;
juxt contributes11 host matches and three guest judgments. No assertion in a
previously loaded namespace changes. The ifn? namespace now stops at unresolved
var, rather than unresolved ifn?. Every remaining failure stays recorded, with
no primary or skip changes. The original baseline-comparison101 receipt remains;
the subsequent comparison against the reviewed actual baseline returns0.

The surrounding45 Rust tests, Java/Node-free double bootstrap generation,
compiler identity and both manifests pass in focused-gates.json. The24-case
basic matrix includes fresh boolean controls for the four opaque BigInt suite
recordings. Twelve behaviors each run on a raw long-parameter and a boxed path:
=, hash, exact Ratio comparison, normalized-ratio ==, positive/negative
promotion, identical?, rejected BigInt/Ratio bit operations, wide/negative exact
comparisons and checked overflow. Wasm signature inspection confirms the raw
variant has an i64 parameter and its boxed variant does not; both are executed
in Runtime/Macro and actual AOT (72 observations). The initial inspection
forgot to install the shipped core environment; its genuine unresolved =
failure and passing actual AOT control remain in initial-inline.*. The fixed
inspection uses the shipped core environment. final-input-diff.json verifies
that only this new regression target and the reviewed baseline differ from
prior surrounding-gate and full-capture inputs.
