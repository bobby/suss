# Retained public scalar hashing

The complete pinned core.cljs1054–1090 hash form is extracted with its EPL1
notice and source SHA256. The explicit fixed-defn patch retains every cond
branch and its order. Numeric finite/safe-integer/floor/remainder operations use
existing private scalar adapters; number infinity/NaN handling uses the reviewed
scalar case bootstrap. String hashing retains live hash-string/Murmur helpers.
Direct IHash wins before primitive cases; default identity uses the retained
extension and root-object special case. No hashing algorithm is replaced.

The Date branch remains present. Original BootstrapDate storage uses the existing
shared nominal descriptor/object layouts and an Object valueOf method. An internal
numeric milliseconds constructor calls a checked scalar TimeClip intrinsic.
The [ECMAScript TimeClip rule](https://tc39.es/ecma262/multipage/numbers-and-dates.html#sec-timeclip)
rejects nonfinite values and magnitudes above8640000000000000, truncates finite
milliseconds and normalizes zero. The intrinsic accepts Number storage only;
unsupported string/object constructor inputs produce a language exception.
It allocates no linear-memory buffer and changes no shared ABI layout.

This is an explicit host-storage adaptation, not general JS Date interoperability.
Calendar/timezone parsing, Date mutation, reader instants, complete Inst and Date
equality/comparison/printing remain pending. Bootstrap declaration privacy and
runtime Var metadata are unfinished: internal names currently resolve through
ordinary namespace bindings, like existing loader helpers. Full collection hash
composition, compiled macro bootstrap and M2–M9 release gates remain required.

Initial regression30006 failed101 on unresolved Runtime hash,
/private/tmp/suss-public-hash-initial-regression.log. First implementation75868
failed101 because private definition attributes are unsupported; internal helper
attributes were removed and that limitation remains explicit. Next43270 and
localized90709/32208 failed101 on generated fragment validation: the new TimeClip
export was missing its unary import signature in the emitter. Adding that
signature fixed the validation defect. Third59391 ended0/native3; strengthened
Date49538 ended0/native3. These failures were not ignored or converted to nil.

Fresh83231 ended0:42 exact pinned/native observations and4 native tests,
/private/tmp/suss-public-hash-primary42.log. Inputs include signed zero, safe
integer modulus, fractional/subnormal/non-safe numbers, infinities/NaN, empty and
UTF16 strings, captured/qualified calls, custom direct IHash, default/root owners,
selector effects and clipped/invalid Date milliseconds. Development-only date-ms
uses reader conditionals to select the pinned host Date or the native numeric
storage adapter. Expected values were provisional until this fresh reference
and independent raw Number/Boolean decoding passed. Root-obj access produces the
expected development oracle private-var warning; no shipped JVM/Node dependency.
Native Date guards also cover live valueOf and IHash priority, typed rejection,
throw recovery and forced GC. No full public compatibility acceptance is claimed.

Core selection98/artifacts102;193 partial reviews/872 unassessed. Source extraction,
patch provenance, sequence setup5 and diff checks pass. Parent47829 terminal0: case5/default3/identity4; Python82 pass. Full61362
is running /private/tmp/suss-public-hash-full.log; independent review and exact reviewed-head CI gate any PR readiness.

Commands (shared target/build2; no RUSTFLAGS):

```
sh scripts/test-public-hash-oracle.sh
cargo test -p suss-cli --test portable_public_hash --locked -- --test-threads=2
python3 -m unittest discover -s scripts -p 'test_*.py'
python3 scripts/core_import.py --check
python3 scripts/sequence_provenance.py
cargo test --workspace --locked -- --test-threads=2
```

Next finish validation/review, then retained ordered/unordered collection hash
composition and complete remaining Date/core/collection/macro acceptance. Deferred
algorithm evaluation remains issue98; no roadmap issue closes from this slice.
