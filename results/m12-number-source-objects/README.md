# Source primitive and Object numeric contexts

The10-control pinned Clojure1.12.6 primary records literal and computed NaNs in
primitive locals, ordinary Object locals, generic calls, lists and vectors, plus
ordinary Double identity. Raw source/stdout/JAR hashes and actual exit0 are kept
under primary; no ClojureScript object-identity expectation is used.

The first native regression on the cache-corrected f1a011e3 source returns101
in91.27s. Four cases differ in each native phase: literal-list-elements,
object-local-containers, computed-object-containers and raw-direct-identity.
An independent retained storage decoder observes the bool/vector results after
GC. The actual compiled AOT component fails against the same complete10-control
primary vector. Two independent decoder adversarial checks pass.

The complete frozen per-command receipt and actual output are in
../m12-numeric-literal-identity/site-partition-cache-and-number-before-corrected-fixture.
The earlier existing-binary probe also reports four discrepancies; it is distinct
from these native Decoder/AOT results. No implementation pass is claimed yet.

The source correction must preserve primitive versus Object comparison/boxing,
source literal object lifetime and original function constant pools; a NaN-only
verdict exception does not implement those rules. The original142-row eq/not-eq
check remains strict and requires its own passing result.

## Source boxing and Object constant execution

The source pass distinguishes Clojure primitive expression kinds from physical
raw/boxed storage. Generic Object uses of primitive floating locals create fresh
source Objects; existing Objects retain their actual references. Numeric literal
Object uses load module-owned original-function constant roots. Symbolic
NaN/infinity constants share that original function's pool; distinct finite
literal sites and exact numeric literal sites remain distinct. The IR's shared
source-site token survives inlining, and artifact keys encode its alias partition.
Only deterministic indices are emitted. Numeric fields, shared ABI, wire encodings,
dependencies and Java-free runtime authority are unchanged.

Generic equality first tests actual reference identity. Genuine source-selected
two-operand primitive equality keeps its numeric overloads, including mixed
Object/primitive consumption and hint validation. Floating identity consumes
actual Objects; integral scalar and UTF-16 identity retain their platform rules.
No NaN verdict is hard-coded. Object-array traversal must run even when its only
child is a local: the settled lexical source facts supply that local's type.

The first compile attempt returns101 on four Rust name-shadowing errors; the
second returns101 on bootstrap Wasm validation after a missing block terminator.
Both exact input overlays and actual receipts remain retained. After fixing those,
compilation/bootstrap pass, but native/AOT checks return101 on two primitive-local
vector cases. The original four counterexamples already match. The verified
pre-correction source and complete failure remain under
source-boxing-after-identity-block-end. Later target/body commands did not execute.

After visiting Object arrays with only local children, the frozen command has
three actual successful results:
- cargo check, including bootstrap generation/validation: 0 in46.66s;
- Runtime/Macro independent decoding and actual AOT targets: 0 in579.39s;
- strict complete numeric source bodies: 0 in166.88s.

The native command runs7 complete-target tests: all16 source-context controls in
both phases and actual AOT batches10/4/2,28 selected source/float/exact controls
from the intact34-case primary, and actual retained roots for8 floating/exact
literal categories across calls/fragments/GC in both phases. Six Long scalar
representation controls in the research recording retain separate platform
evidence; no all34-match claim is made. Two adversarial decoder checks also pass.

The complete original eq/not-eq bodies retain all142 ordered identities and have
142 host matches, zero failures, guest judgments, skips or unexecuted rows. The
complete default num body retains all26 identities, with23 host matches and the
three explicit Short/Byte/Integer platform differences; none is called a host
match. The original global primary, counts, comparator, skip/opaque rules and
upstream suite bytes are unchanged.

The supplemental6-control capture primary returns0. Both before-correction CLI
probes (combined6 and unhinted4) already agree, so these are regression coverage,
not additional negative counterexamples. Raw JVM source/JAR/stdout hashes and
native observations are retained. All current source and HEAD hashes remain
unchanged through the actual terminal batch. Broader compiler/predicate/hint/WIT/
budget/cache/bootstrap checks, full workspace, independent review and final-head
CI require separate evidence.

## Typed numeric consumption and legacy predicate controls

A focused emitted-loop check exposes an allocation after the semantic correction:
a known local typed Double function received an eagerly materialized Object on
each iteration. Its corrected test discovers the shared immutable one-f64-field
box type instead of accessing a private Rust constant. The earlier test setup
compile error is retained separately and is not the semantic counterexample.

The actual before check returns101 in2.14s. Definite floating input now remains a
distinct logical Object SSA value represented raw until Object materialization.
A typed numeric consumer can use that raw value; aliases of the logical Object
retain their eventual cache and separate source Objects retain separate SSA
values. Other inputs keep their boxed path. The allocation check returns0
in172.14s including recompilation, with source/HEAD unchanged. Native identity
and strict body checks must be re-run after this representation change.

The new10-control Clojure primary under box-reuse-primary independently checks
all4 original box-reuse programs and floating predicate contexts. One legacy
Suss-only expected result changes from [1.75 1.75 1.75 true] to the actual JVM
[1.75 1.75 1.75 false]; the other3 programs are unchanged. Direct symbolic NaN
identity is true, primitive-local NaN identity false, Object-local NaN identity
true, and separate finite/signed-zero Double identity false. Updated tests retain
all prior programs and add the observed distinctions. These corrections replace
the old JavaScript-style Double value identity; Long/UTF-16 scalar choices remain
explicit. No original suite, primary count/comparator or opaque/skip rule changes.


### Deferred source Objects, exceptional printing and atom identity (2026-11-24)

A typed local Double call exposed eager Object allocation inside its numeric
loop: the corrected emitted-code check first returns101 in2.14s, then0 in172.14s
after deferring physical materialization of the distinct source Object SSA value.
The first broader check finds a missing raw-consumer classification and panics;
its exact pre-correction source is retained. After that fix all20 raw primitive/
local/capture layout tests pass, along with Object and box-reuse targets.

The next actual failure is computed negative infinity printing -Infinity instead
of ##-Inf: literal reference identity no longer stands in for numeric comparison.
The printer now uses numeric comparisons for both infinities. Atom identity drops
the obsolete JavaScript NaN fallback. Eight fresh Clojure1.12.6 controls retain
computed exceptional printing and primitive/shared/distinct NaN atom behavior.

The frozen deferred-object-consumption-after-printer-fix gate returns0 for all
eight commands, source and HEAD unchanged. It runs6 capture/atom tests (original18
capture programs plus8 new primary controls in Runtime/Macro and actual AOT),
15 Object/literal/predicate/box-reuse target tests, strict complete142-row original
eq/not-eq bodies and all26 default-num rows (23 host matches, the same three
explicit narrow-number differences), Java/Node-free double bootstrap and4
bootstrap execution tests, core-import/sequence/numeric provenance and unchanged
upstream contract. Individual actual receipts, earlier failures, source overlays
and independent primary outputs are under results/m12-number-source-objects.

Public num inline boundaries still need their own regression/correction. Full
numeric issue acceptance, exact workspace, independent review and final-head CI
remain separate gates; original suite/primary/count/skip/opaque rules do not change.
