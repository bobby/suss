# Scalar constant declaration evidence

Clojure1.12.6 is the primary oracle. `cases.json` retains12 top-level-form
inputs, complete JVM stdout and expected final values. `oracle.json` records
the pinned JAR hash and command; `oracle.clj` collects the inputs. Each input
was run in a fresh JVM. The CLI regression executes the same source in Runtime
and Macro, and compiles all cases into a WIT component whose check export must
return1. Values remain rooted through collection before host inspection.

`before-runtime.json`/`.log`: terminal101, both Rust tests fail on the original
unsupported :const diagnostic. `after-first.json`/`.log`: terminal0, both tests
pass, frozen source unchanged. These receipts include commands, source hashes
and fixture hashes. The earlier `before` attempt fails at scope setup because
XDG_RUNTIME_DIR was absent; it executed no tests. The driver now sets only that
session-bus location alongside the existing two-job/stable/isolated-target setup.
No developer RUSTFLAGS are overridden.

The original do-wrapped probe did not establish frozen readers from earlier
top-level definitions. Its unmodified source/output/metadata remain under
initial-do-*; the executed regression uses actual top-level forms instead.

The implementation accepts scalar literal :const initializers and scalar
constant references already resolved at the top-level form boundary. It rejects
computed/collection constants and nested definitions involving constants, whose
runtime initialization requires additional work. False/nil :const flags are
ordinary live definitions. Tests retain these explicit limits instead of
pretending a live var is a constant.

The constant tests do not prove the vendored numeric-suite criteria; those
require executing the unchanged suite and examining the retained raw capture.


The first suite capture is terminal101 after587.8s because the baseline changed:
3714 host matches,54 guest-judged,459 failures,2712 unexecuted,92 namespace
failures.29 namespaces now load. The audit finds zero changed assertions in
previously loaded namespaces. The proposed baseline is an unselected review
artifact; the checked-in known-failure baseline has not been changed.

The first literal-substitution implementation's `identity.json` records JVM
[true true true] versus Suss[false false true] for BigInt/Ratio/string constant
reads. The bare-literal control is[false false] on both JVM and the prior4cf
CLI, so value-based literal pooling would be incorrect. before-root.patch and
before-root-test.rs reconstruct that earlier implementation. The private-cell
revision passes18 cases in both phases and AOT; its three primitive diagnostic
mismatches remain in primitive-constant-probes.json and before-source-facts.patch.
The source-facts correction then passes both value/AOT tests and all three
negative compilation cases in both phases (after-source-facts.json).

The final focused fixture has19 JVM cases,57 Runtime/Macro/AOT observations,
and exact diagnostic-fragment checks in both phases. final-focused.json is
terminal0; oracle-check.json independently reruns each case on fresh JVM1.12.6.
Final suite/declaration/Java-free bootstrap gates have separate labeled receipts
and must be checked for terminal status. Earlier suite captures do not certify
the private-cell/source-facts revision.


The final reviewed baseline has now been selected, retaining every actual
failure and changed namespace diagnostic. `repair-gates.json` records the
passing Python comparison and35 comparator tests. The stale scalar-const
unsupported test is corrected to an unsupported computed initializer; its
original failure remains in final-gates.json. Its rerun passes13 definition
and14 resolution tests, completing47 scoped Rust checks including the14 CLI
declaration tests,4 bootstrap tests and2 constant tests. Bootstrap images
reproduce twice without Java/Node, and both source/import manifests pass.
`repair-input-diff.json` lists only the stale test and reviewed baseline: no
production input differs from the completed full capture/bootstrap gates.

The vendored source provenance remains jank-lang/clojure-test-suite at
95d4a9112cfc5fe6629be3d14bb99080ea00af2f, under its retained upstream license.
The primary oracle is unchanged. The old reviewed baseline is pinned in
reviewed-before-known-failures.json.gz with its commit/hash in baseline-source.json.
Run audit-suite.py [final] to reproduce the delta against that exact baseline;
it reads archived gzip captures when raw generated JSON is absent. Raw capture
and per-assertion audit rows are archived as gzip; fresh runs retain JSON too.

Large command receipts are archived byte-for-byte as `.json.gz`; the logical
`.json` names above are what fresh runs write. Decompress the archive to inspect
its complete frozen input hashes. No fields or failed commands are discarded.
