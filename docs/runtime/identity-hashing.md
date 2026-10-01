# GC-owned identity hashing and ABI2

Pinned default IHash at core.cljs1492–1497 uses Closure getUid, with a separate
root-object zero special case. Portable ordinary objects and functions therefore
need stable per-value identity. A global reference table would keep dead hashed
objects reachable; object addresses are not stable across moving GC. This slice
adds an original private owner-held UID adapter; it does not select public hash
or the default IHash setup yet and does not replace the pinned Murmur algorithms.

The dated design entry advances the runtime ABI from1 to2. The same ten-type
recursive group appends a mutable Value UID slot to Closure4 (index4), Descriptor6
(index4), UserObject7 (index3) and Error8 (index4). All older field indexes stay
unchanged, including metadata/message/data/cause. Constructors initialize nil;
only the identity adapter lazily assigns a boxed positive safe integer. The
allocator's global is an i64 counter with no owner references. An owner roots its
UID, and the UID has no back-reference to the owner. Source fields, captures,
protocol tables and metadata remain independent of identity storage. Counter
exhaustion at the safe-integer limit is an explicit bounded resource error.

The adapter rejects unsupported scalar/internal kinds and malformed UID storage
with typed language errors before casts. Repeat reads return the same Number
reference; separate owners get separate IDs. No scalar/object coercion is used.
This UID is local to a runtime, not a persistent content fingerprint. Hash width
coercion, equality composition, default dispatch/root-object handling and public
hash are still subsequent source work; no algorithm selection under issue98.

Artifact manifests now require ABI2. The frozen ABI1 recursive group from
reviewed8e0f3b5 is retained only as a development fixture. A real ABI1 caller and
an ABI1 layout falsely labeled ABI2 both fail before initializer effects. Engine
linking independently rejects their incompatible imports. Old artifacts require
recompilation. No second compiler/runtime ABI is shipped. The original ABI1
layout/evidence remains historical in abi-v1.md.

Fresh18 primary observations initially passed while native failed on unported
with-meta. Rather than alter that certified source, retain whole IFn586/MetaFn2112
and original with-meta2165/meta2174 bodies with explicit defn patches and EPL
provenance. All IFn signatures and MetaFn methods remain intact. Highest apply
arity, general IFn function-call lowering and complete ordinary function metadata
remain unfinished; `apply` stays declared uninitialized. These source imports
are partial, not completed core/metadata acceptance. Selection96/artifacts100,
reviews189 partial+876 unassessed. The original18 observations remain unchanged.

Evidence so far:

- Initial regression82402 ended101 on unresolved private identity-uid; focused
  implementation66255 ended0 native1.
- ABI63405 ended101 with eight old-layout/manifest fixture failures; ABI64950
  ended101 with one remaining old error field-count assertion. Fixtures now
  initialize/inspect the added UID, preserving prior data/error checks. ABI37353
  ended0 all42 required tests.
- Fresh26855 ended101: primary18 exact, native missing with-meta. After complete
  source dependencies, fresh40264 ended0:18 exact primary/native2.
- New corruption guard28207 and its first correction failed to compile on wrong
  Wasmtime reference-comparison API/reference borrowing. Corrected test to the
  existing Rooted::ref_eq API; guard84913 ended0. All four owner families reject
  null/false/zero/negative/fractional/NaN/infinite/unsafe UID payloads without traps,
  with GC and cached-reference restoration.
- ABI1 gate77421 ended0, including actual engine linking and zero initializer
  effects. Subsequent fresh26/native3 results are recorded below.

Commands use shared target/build2/--locked/--test-threads=2, no RUSTFLAGS:

```
sh scripts/test-identity-hash-oracle.sh
cargo test -p suss-cli --test portable_identity_hash --locked -- --test-threads=2
cargo test -p suss-compile --test runtime_abi --locked -- --test-threads=2
python3 scripts/core_import.py --check
python3 scripts/sequence_provenance.py
python3 scripts/cljs_reviews.py
cargo test --workspace --locked -- --test-threads=2
```

Independent PR review/fixes, reviewer full baseline and exact reviewed-head CI
remain required before readiness. No merge, issue or milestone closure. Complete
public/ordered/unordered hashing, remaining collection types, metadata/transients,
compiled macros, printing and M2–M9 release gates remain open.

Fresh8773 terminal0:26 exact primary/native2, /private/tmp/suss-identity-hash-primary26.log;
original18 unchanged. Focused88306 terminal0: native identity3/indexing3/sequences6/
persistent-session suites pass with forced GC, typed errors and both phases.
Python80/import100/setup4/reviews189+876/diff pass. Required full baseline and
independent review/final-head CI remain pending; root owns heavy slot.

The first full workspace run86115 ended101:11 closure tests passed and3 failed
because their shared raw Error decoder still expected four ABI1 fields. Updated
the decoder to require five fields and inspect nil data/cause/UID, preserving
message, effect and recovery assertions. Focused34964 ended0:14 closure tests
passed. Full retry4743 ended101 on another stale Error decoder in the definition
suite (12 passed,1 failed). A broader decoder search found the same old count in
namespace resolution. Both now require five fields and inspect nil data/cause/UID
while retaining all previous assertions. Focused74630 ended0:13 definition and11
resolution tests passed. The next full baseline is running in
/private/tmp/suss-identity-hash-full-final.log; no full pass is claimed yet.
