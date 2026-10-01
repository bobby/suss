# Retained Object methods

The checked bootstrap now lowers user-type Object blocks and direct dot calls.
Forty-four fresh pinned observations match independently decoded validated Wasm
with forced GC; the original24 probes are unchanged. Two native tests cover the
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
host attributes remain explicit unsupported paths. This bounded adapter supports
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
