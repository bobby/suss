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
