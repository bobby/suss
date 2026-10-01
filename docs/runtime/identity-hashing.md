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
resolution tests passed. Full65381 ended0 through final doc tests, including all44 ABI regressions,
/private/tmp/suss-identity-hash-full-final.log. Existing ignored/manual tests remain
explicit. Independent review, reviewer baseline and final reviewed-head CI remain
pending; this is not public hash or milestone acceptance.

## Independent PR107 review — 2026-10-01

Reviewed262b2d7 and root evidence160ccdc against reviewed1068e0f3b5 in isolated
/private/tmp/suss-review-pr107 with actual detached upstreamc4295f30. Read accepted
design/ROADMAP/inventory/handoff. All production constructors initialize appended
UID storage; scalar-only allocator retains no owner references. Reviewed cached
malformed guards, manifest/prelude gates, frozen ABI1 linking, private HIR/IR
arity and complete licensed metadata source forms.

Review found a significant canonical IFn receiver/arity dispatch defect. The pin's
emitted nominal IFn methods omit the physical receiver formal, whereas explicit
-invoke passes its target in the JS operand list. Generic protocol dispatch had
therefore selected the wrong source arity. Canonical suss.core/IFn now has a
private dispatcher carrying the next source-arity key; nominal dispatch prepends
the already evaluated target value. Ordinary protocols, including unrelated user
protocols named IFn/-invoke, retain their receiver-inclusive convention. Native
fallback still uses the current method cell. No operand is evaluated again and
no generic fixed-function arity rule changed. The extra runtime constructor
checks its next key before use and rejects malformed storage with language errors.

This correction exposed a false positive in original metafn-explicit-invoke.
Its unchanged source is `(let [w (with-meta (fn [] 17) false)] (uid w)
(= (-invoke w) 17))`; its unchanged exact pin observation is true. Correct pin
receiver dispatch calls the underlying zero-arity fixed function with the target.
Emitted JS ignores the excess argument; Suss's accepted strict invocation contract
requires a typed Error whose independently decoded message is exactly Wrong arity.
That case is now asserted as a separate exact contract divergence. It is never
skipped, rewritten or counted as a matching value. Original26 now comprise25
matching relations and1 typed contract boundary. Their JSON source/expectation
objects and all parent corpus bytes remain unchanged. General callable-object
syntax, highest apply arity and full function metadata remain unfinished.

Eight appended review probes cover mixed owner allocation, array mutation,
caught Error identity, static IFn target operands/metadata, wrapper copies and
metadata payload identity. Final fresh29279 ended0:34 exact primary observations,
33 matching native relations plus1 exact boundary across4 native tests,
/private/tmp/suss-pr107-review-oracle34-final.log. The fourth native test checks
operand order once, thrown target prevents later effects, strict arity error,
GC/UID recovery and unrelated IFn protocol dispatch. Existing source dependencies
are retained intact; no arity-normalizing intrinsic or source method deletion.

Failure history remains explicit. Initial fresh68141 ended1 on two provisional
true expectations; generated pin JS revealed receiver semantics. Native95912
ended101 on confirmed mismatch after preserving their actual false observations.
Those new provisional source candidates also relied on JS excess argument/object
coercion behavior outside this slice; corrected only those new candidates to
callbacks accepting the actual argument list, preserving the original26.
Fix51693 ended101 on private Global.name access; corrected accessor. Fix26689
ended101 on a new callback's missing eqref-to-ARGS cast. Corrected cast;
fresh31023/4452 primary certification succeeded but native ended101 on the
unchanged zero-invoke strict-arity divergence. Recording that exact boundary
instead of altering strict semantics made native93996 pass4; final29279 freshly
certifies all34 and native4. Logs preserve every initial failed run.

New ABI review fixture exports only the final scalar global in development bytes
without replacing runtime instructions. It exercises MAX_SAFE_INTEGER allocation,
first exhausted allocation's typed error, unmodified nil owner slot/counter and
cached-owner GC recovery. Initial92068 ended101 on test-only wasmparser u64 range
indexing; corrected explicit usize bounds,61754 ended0. Focused46577 ended0 all45
ABI tests including new constructor corruption guard; full baseline also checks
the final strengthened valid-old-key/invalid-next-key fixture.
Python90053 ended0:80 tests, /private/tmp/suss-pr107-review-python.log;
import100/setup4/reviews189partial+876unassessed/diff checks pass. Commands use
shared target/build2/--locked/testthreads2 and no RUSTFLAGS.

Required independent full90116 is running in /private/tmp/suss-pr107-review-full.log.
Reviewer retains heavy slot until authoritative terminal output and all handles
are inspected. Exact final reviewed-head CI remains required before readiness.
No merge, issue closure or milestone acceptance; public/default/collection hashing,
printing, remaining collections/compiled macros and M2–M9 gates remain unfinished.

Independent full90116 ended0; inspected /private/tmp/suss-pr107-review-full.log
through final reader doc tests. All enabled required suites pass, including
identity4/33matches+1 exact boundary, ABI45 with strengthened invalid-next-key
and allocator exhaustion fixture, and unchanged parent corpora. Source receiver
reference is pinned core.cljc1509–1540 (adapt-ifn-invoke-params and
ifn-invoke-methods), with canonical IFn selection at1587. Final Python40584
ended0/80, /private/tmp/suss-pr107-review-python-final.log. All reviewer handles
92068/61754/68141/90053/95912/51693/26689/31023/4452/93996/29279/46577/40584/90116
are terminal. After review fixes/evidence commit/push, heavy slot is released.
Exact final reviewed-head CI remains root readiness gate; no merge/closure.
