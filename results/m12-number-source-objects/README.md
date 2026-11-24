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
