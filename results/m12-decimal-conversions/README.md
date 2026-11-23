# Decimal conversion and integration evidence

Fresh primary controls use pinned JVM Clojure 1.12.6; shipped code has no Java
or Node dependency. `oracle.py`, `context-coercion-oracle.py` and the recorded
commands/JAR hashes identify the development oracle inputs. Original failed
runs are retained alongside repairs, without substitutions or skips.

`before-exact-conversions` reports 46 mismatches in each phase against 137
controls: the original 34 Decimal conversion failures, widened with BigInt/
BigInteger float midpoints and ordinary-function/alias controls. Exact low bits
repair unchecked integer casts; checked casts retain Clojure's intermediate
Double range semantics. A private UTF-16 scratch parser rounds exact canonical
text directly to f32 and widens once, avoiding double rounding. `rationalize`
uses exact Decimal text independently of the dynamic precision context.
`numeric-reproduce.log` records a byte-identical helper rebuild.

`after-exact-conversions` passes all 137 controls in Runtime, Macro and real CLI
AOT. `float-fields-cases.json` additionally records 13 primary raw Float bits
and corresponding widened Double bits; both phases independently decode those
actual bits and unchanged exact Decimal fields after GC. The standalone helper
regression includes malformed input, midpoint and scratch-boundary checks.

`before-unused-context-fix` exposes two failures per phase in eight additional
primary controls. Pure integer-to-Decimal coercion and Decimal comparison must
ignore an invalid unused MathContext; Ratio conversion/comparison and arithmetic
still inspect it. `after-unused-context-fix` passes these eight controls in both
phases and all 145 combined conversion/context controls in actual CLI AOT.

CI run45108989683 at477a5d76 fails four source-core reload tests and the AOT
size guard; the complete three failing job logs are retained. `ci-repair` records
all five passing unchanged tests, plus retained Decimal functions reading live
dynamic bindings across core reload, GC and a deliberately failed initializer in
both phases. Registration replaces the phase-owned getter after closure
validation. An initial experimental harness tried raw source reload on top of
the direct-linked shipped image and failed with unresolved private slot imports
both before and after the repair; those receipts are preserved. The final reload
regression loads and reloads source core in minimal phase-owned stores, as the
original four failing tests do. It does not claim the experimental route works.

AOT provenance now omits optional EDN padding while retaining all fields,
strings and conservative macro dependencies. The unchanged size test records
3895 bytes/form (limit3900), with96 extra code bytes per split form (limit128).
Identity/integrity tests include escaped paths, non-ASCII strings, unknown versus
empty dependencies and stale/corrupt artifacts. `compact-edn` records Clojure
EDN reading all34 real component manifests, checking print/read identity and
comparing unchanged fields and every macro identity against the original
spaced manifests. Initial proof-harness assertion/syntax failures are retained;
the corrected proof distinguishes bootstrap-only and full source macro graphs.
`probe-aot-size.rs` is the preserved temporary diagnostic example; it is not a
shipped crate target.

`prior-full-workspace` retains the unfiltered c9d1c6e command, unchanged source
hashes, and terminal101 after2784 seconds at the same source-core reload failure.
No full workspace success is inferred from focused tests. Complete capture,
broader gate and later full workspace receipts qualify their own frozen inputs.

The separate13-case precision-types oracle is exploratory primary evidence,
not a passing Suss fixture. String/nil MathContext constructor diagnostics and
other complete BigDecimal acceptance gaps remain outside these conversion
claims. `archive.py` produces deterministic gzip copies of large receipts and
artifacts; `archives.json` gives original byte counts and SHA256 hashes. Raw
local copies are not additional independent runs.
