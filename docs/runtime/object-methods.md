# Retained Object methods

The checked bootstrap now lowers user-type Object blocks and direct dot calls.
Fifty-two fresh pinned observations match independently decoded validated Wasm
with forced GC; the original44 probes are unchanged. Two native tests cover the
corpus and malformed declaration/compile-atomic recovery.

The class descriptor owns named keys and unbound method functions. Instances of
one class share the method value; old instances and captured class values retain
old methods after class redefinition. Runtime extension updates the descriptor.
Storage uses existing Descriptor/UserObject/Closure/Args types; private tags and
the portable implicit-this realm append globals without moving existing indices.
There is no owner registry or shipped Java/JS dependency. Private argument buffers
are not persistent collections. Ordinary protocol dispatch and native property
storage remain separate and keep their existing semantics.

Direct dot calls evaluate the owner, then lookup, then arguments once in source
order. Null owners fail before arguments; missing/noncallable methods on a known
instance fail after arguments. Object methods receive anchored physical this and
recur replaces only user parameters. Single fixed methods fill missing arguments
with undefined and ignore evaluated extras; multiple signatures reject gaps.
Detached method values use the portable non-strict implicit-this realm rather
than binding an instance; reading a missing numeric field then produces NaN.

Retained pinned Fn and fn? add two source selections. Fn is an unchanged marker;
fn? has an explicit hash-bound defn bootstrap patch preserving its marker branch,
short circuit and docstring. The original js-fn? primitive recognizes shared
closures. EPL/source ranges/notices remain generated and checked. Current source
selection is28 forms/32 licensed artifacts;82 partial reviews/983 unassessed.
This does not establish compiled macro or complete callable collection support.

Source provenance is pinned core.cljs and core.cljc adapt-obj-params/add-obj-methods
at c4295f303100bbf5afac449242d30bca1126f1a1. Complete List/EmptyList/Cons/IndexedSeq
Object blocks must remain intact when porting those collections. No upstream macro
source is copied by the original lowering/runtime helpers or development corpus.

Public prototype access/replacement, dynamic extra instance fields, computed or
munged names, default-realm property mutation, primitive Object extension and full
host attributes remain explicit unsupported paths. Object __proto__ declarations
raise a located compile error: pinned prototype assignment invokes its inherited
setter and requires prototype mutation rather than ordinary method storage. Raw
__proto__ fields also raise a located compile error, including protocol bodies
that would otherwise bypass named storage. Host-created __proto__ schemas reject
named get/set explicitly. Pinned constructors assign this.__proto__, invoking the
same setter. This bounded adapter supports
retained user-type methods; it is not a claim of general JS interop or full core.

Command:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-object-method-oracle.sh`.
Fresh graph88197 ended0:44 exact primary observations, enabled native corpus pass.
Log /private/tmp/suss-object-method-final44-primary-second.log. The pin's generated
arrow replacement and protocol recur warnings remain visible; generated JS confirms
Object recur updates n/acc while retaining this. The first expanded graph88291
matched all44 primary values, then native failed on an unimplemented general =
in the new recur probe. The probe now uses supported <=; original24 remain intact.
Historical preparation evidence follows; it does not describe current support.


First fresh graph96832 ended1 at strict comparison, before native execution.
All24 observations were produced. Two provisional expectations failed: a detached
score method returned canonical NaN rather than catch17, and a type declared in a
let did not capture factor (undeclared namespace var warning, NaN rather than35).
Logs /private/tmp/suss-object-method-primary-candidate.log and its exact generated
JS confirm both paths. The detached expectation now records actual NaN bits. The
lexical probe now declares a global11 and shadows it with local7, to distinguish
namespace lookup from lexical capture without relying on an undeclared var. That
new expected55 still needs fresh certification. No failed expectation counts as
success and native Object support remains untested.


Fresh95816 completed terminal101:24 exact primary observations, followed by located
native unresolved Runtime name Object at43..49. Log
/private/tmp/suss-object-method-primary-native-red.log. Command:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-object-method-oracle.sh`.
The global11/local7 discriminator returns55, confirming namespace lookup rather
than an outer lexical capture. Intentional ->ObjectProbe replacement warning stays
visible; the revised lexical discriminator has no undeclared-var warning. Native
method implementation, receiver storage and full prototype surfaces remain open.


Independent PR #93 review adds eight source probes without changing the original44.
Fresh session73059 ended0 with52 exact primary/native observations. It checks
duplicate arity last-wins behavior, shared detached implicit-this identity, nested
receiver captures, properties on method wrappers, noncallable own-field argument
order, parallel recur bindings, parameter shadows, and Object call versus Fn.
Log /private/tmp/suss-pr93-review-primary-final.log. Pinned duplicate-arity and
protocol-recur warnings remain visible. A discarded exploratory js-fn? value probe
produced macro-value warnings; the retained Fn probe uses the public fn? contract.
An initial provisional wrapper result was corrected from124 to24 before final
certification; old instances correctly retain their original class methods.

The independent malformed-storage test rejects opaque tail keys despite matching
prefixes, tagged keys with non-string names, and corrupted wrapper payloads using
checked language exceptions, without Wasm traps. Focused session76342 ended0.
Log /private/tmp/suss-pr93-review-abi-guards-final.log. The first guard-test compile
used an unavailable Wasmtime StructRef setter; the final fixture mutates an owned
Args table through its supported API.

Final guarded source graph17758 ended0 with52 exact primary/native observations
and both native tests. Log /private/tmp/suss-pr93-review-primary-final-head.log.
A separate pinned diagnostic namespace confirms __proto__ primitive initialization
has no own field and does not read7, while an Object __proto__ method changes the
prototype to a function. This establishes an unsupported boundary, not three
additional matching corpus cases. Log /private/tmp/suss-pr93-review-prototype-primary.log.

A foreign shared-ABI closure copying the detached callback with nil or wrong-tag
environment exposed a recursive fallback and Wasm call-stack exhaustion. Red
session2438 ended101; log /private/tmp/suss-pr93-review-detached-wrapper-red.log.
The detached callback now checks its private UserObject/tag before reconstruction.
Fixed ABI session31161 ended0 with20 tests: both copied environments and corrupted
payloads raise checked language exceptions without traps. Log
/private/tmp/suss-pr93-review-abi-wrapper-fixed.log. Valid-source behavior and the
ten shared GC prelude types remain unchanged.

Fresh fixed-wrapper graph37647 ended0:52 exact primary/native observations and
both native tests. Log /private/tmp/suss-pr93-review-primary-wrapper-fixed.log.
A separate pinned reserved-name diagnostic confirms null/constructor Object
methods stay raw for direct calls and named function reads (13/true and29/true).
Compiler emit-dot uses an empty reserved set for these property paths, unlike
constructor field munging. No blanket reserved Object-method guard is justified.
Log /private/tmp/suss-pr93-review-prototype-reserved-primary.log; these diagnostic
observations do not expand the52 matching native corpus.
