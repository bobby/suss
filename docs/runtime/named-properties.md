# Named-property dependency preparation

This is an original reference corpus and source audit, not a property
implementation or compatibility acceptance. Retained List/EmptyList/IndexedSeq
source uses List.EMPTY and UTF-16 string length. Property publication must retain
owner identity and remain separate from ordinary function environments, native
protocol tables and source nominal field schemas. No host JavaScript runtime may
be added to shipped execution.

Twenty-two scalar observations now match fresh ClojureScript execution at
c4295f303100bbf5afac449242d30bca1126f1a1. They cover missing/published class statics,
class aliases, assignment values, source instance fields, closure-owned properties,
retained function/class owners after replacement, ordinary calls, UTF-16 length,
mutable-array growth, once-only ordered owners/values and dynamic nil/error
recovery. The native regression decodes f64/Boolean values independently after actual Wasm
execution, but currently fails on unresolved .-EMPTY before any property case can
execute. No property behavior
is certified in Suss by these reference observations.

The first candidate reference build38134 ended1 before observations because
literal nil dot access is rejected by the pinned analyzer. Its exact diagnostic
was Unknown dot form of (. nil -EMPTY nil). The runtime-error probe now uses an
unknown function parameter receiving nil; this is a different, valid source path,
not success for the rejected literal form. A runtime function replacement uses
set! rather than a redundant def to isolate ownership semantics. Fresh32149
ended101:22 exact primary matches, then the native located unresolved .-EMPTY.
The primary compiler warns that generated ->PropertyProbe is replaced by the
intentional second deftype; this warning is retained in the log.

```sh
CARGO_BUILD_JOBS=2 sh scripts/test-named-property-oracle.sh
```

Log: /private/tmp/suss-named-property-primary-native-red-dynamic.log.
No inventory/review/source-import/license count changes follow from this prep.
General prototype/Object method blocks, coerced/computed property names, array
length assignment and full host interop remain outside these probes. Literal nil
property rejection must remain explicit; the runtime case does not certify it.

Next implement source property lowering and GC-owned storage required by actual
retained forms. Preserve existing closure capture/invoke behavior, native dispatch
and field access, ordered effects and errors. Do not use a root registry that
retains replaced owners, treat invocation Args as persistent sequences, replace
an unknown property with nil, or claim concrete sequence types complete. Source
canonical empty-list literals, Object method blocks, concrete List/EmptyList/Cons/
IndexedSeq, equality/hash/reduction and persistent rest/apply remain unfinished.


Implementation audit: closure-new already wraps source, class and runtime closure
environments in the existing GC-owned wrapper. closure-property-fields returns
its two-element payload (original environment and a key/value table). Current
native entries use i31 kind keys0–7 and compare by RefEq; ordinary UTF-16 property
keys must remain distinct and must not weaken the native helper's kind guard.
This makes sharing owner storage a candidate to validate, not tested support.
Nominal instance fields retain UTF-16 schema names and separate field storage;
malformed schemas/tables must raise language errors rather than missing-property
success. Source-array/string length, known object fields and class/function statics
need executing guards. Arbitrary object/prototype storage and Object method blocks
still need an explicit implementation, not deletion from retained types.
