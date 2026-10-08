# Pinned chunk-demand and lazy-effect observations

Refs #17. This is an authored bounded acceptance slice, not completion of the
original issue or general chunked/lazy core compatibility. Native execution is
pending. Existing lazy, vector and other oracle cases remain unchanged.

Branch `portable/m4-chunk-effects` at `/private/tmp/suss-m4-chunk-effects` starts
from `3c80b606d5db721740d31bb16d99abf5fe4e1cc3`. The ClojureScript reference was
initialized from the existing local core-import checkout, without network access,
and checked out at `c4295f303100bbf5afac449242d30bca1126f1a1`.

## Shared fixture and exact observations

The original shared fixture `tests/oracle/fixtures/chunk-effect-probe.sus` uses
public vector/sequence/chunk accessors, `concat`, `reduce`/`reduced`, metadata and
canonical `cljs.core/LazySeq`. Its explicitly original producer records an ordered
persistent vector of elements processed and a thunk-entry count. Chunked input
uses real `chunk-first`, `array-chunk`, `chunk-cons` and `chunk-rest`; unchunked
input uses `first`, `rest` and `cons`. It is not a copied or substituted upstream
map implementation. Public `map`/`filter` are not retained in this core artifact;
map-specific callback/effect acceptance remains a separate unsupported dependency.
No production definitions, import recipes, reviews or bootstrap artifacts change.

Pinned PersistentVector source uses IndexedSeq for lengths <=32 and ChunkedSeq
above32. Thus the 31/32 cases deliberately realize one element initially, then
one element per additional demand. Length33 forces elements0..31 on first demand;
within-chunk access to31 adds no producer effects, while demand32 realizes the
one-element tail. Length65 forces0..31, then32..63 at demand32, then64 at demand64.
Demand33 and repeated access remain cached. Empty-tail thunk entry is separately
observed for the small unchunked cases rather than confused with an element effect.

The nine strictly ordered cases cover demand at lengths31/32/33/65, retry of a
throwing second chunk, Reduced at indices0/31/32, and concat prefix/chunk crossing.
Complete nested vectors preserve every checkpoint's return value, thunk count and
ordered trace, not merely the final result or number of callbacks. Retry records
32 twice (failed attempt and retry), retains the completed first chunk and exposes
IPending status. Metadata decoration remains unforced initially. Reduced stops
reducer callbacks exactly at the chosen element while preserving the already
realized producer chunk; stopping at31 does not force the next chunk, while
stopping at32 forces its complete producer chunk but no following chunk.

`chunk-effect-cases.json` contains reviewed explicit expectations in lossless
binary64/Boolean/vector tags. The generator embeds the same fixture and expressions
for actual pinned compilation. The comparator rejects changed pins, unknown or
missing fields, duplicate keys, missing/reordered/duplicate identities, malformed
tags and complete nested differences (including effect order and float bits).
A thrown/unresolved/unsupported case aborts the runner; no skip or result wildcard.
The runner has no Cargo invocation and bounds fixture input to65 elements.

## Native tests and evidence limits

`portable_chunk_effects.rs` authors two tests in both Runtime and Macro Sessions:
all nine complete primary observations after GC, and retained lazy state across
source fragments with GC before inspecting each demand/retry result. Actual
canonical storage is decoded through the existing bounded FormBridge; the test
converts only actual numeric bits, booleans and vectors, rejecting all other kinds.
Guest equality and printing do not judge observations. The second test checks
exact traces before failure, after failure and after successful retry, plus cache
preservation. Both use the existing finite100M sequence stress allowance, not a
measured cost or a proven sufficient budget. No fallback/retry/ignore is added.

Native code is uncompiled and unexecuted. Missing source dependencies, wrong
values, traps, transport failures or fuel exhaustion must remain explicit failures
when the coordinator releases the Cargo slot. Primary success alone does not
prove native effect ordering or satisfy all original #17 acceptance criteria.

## Provenance

No upstream implementation was copied in this slice: fixture, runner, comparator
and native tests are original MIT/Apache-2.0 development code. The actual retained
source implementations remain under their existing EPL-1.0 notices and patches;
`runtime/core-import/manifest.json`, extracted forms and reviews hold exact
source/file/adaptation hashes. Key original declarations at the pinned reference:

| Declaration | Source SHA-256 |
| --- | --- |
| `runtime:PersistentVector:5713` | `9208b736d82fa1c0a8649147cc1e30cea5d3fad6d733e805a750744242d4a205` |
| `runtime:LazySeq:3584` | `ec37554b752797e7036784a18eaf322d4ad8491bc90f5148a2f4b8c338f445e4` |
| `runtime:ChunkedSeq:5968` | `08f17ae1e89e79fb17ed94ef40152f5c84585e2a14517b52b2362baa602949b7` |
| `runtime:ArrayChunk:3690` | `333e6efaaf40da955693cafa95b637aa1fef0a27cbf77226217fa1cb539a9126` |
| `runtime:ChunkedCons:3722` | `3c8eda64d3dd81f033add510b10560cb57da15e8637fb96d70c4619211f0b0c2` |

The relevant protocol/helper declarations also remain `:in-progress`; this test
slice does not reclassify them or imply compiled upstream defprotocol support.

## Commands and results

- First pinned run failed because the original fixture incorrectly required
  IChunkedSeq on a31-element IndexedSeq. Retained failure log:
  `/private/tmp/suss-m4-chunk-effect-oracle.log`. Corrected the fixture to retain
  both actual source paths; did not skip short inputs or change any prior corpus.
- `sh scripts/test-chunk-effect-oracle.sh`: exit0, all9 complete observations
  match actual pinned compilation/Node execution; log
  `/private/tmp/suss-m4-chunk-effect-oracle-second.log`.
- `python3 -B -m unittest discover -s scripts -p 'test_chunk_effect_oracle.py'`:
  4/4 pass. A first mutation test reversed a one-element trace (no change);
  corrected that test to reverse the multi-element checkpoint. Strict failures
  now cover deleted/reordered effects, dropped retry effect and signed-zero bits.
- Existing `test_lazy_sequence_oracle.py`: 4/4 pass, old cases untouched.
- `python3 -B scripts/core_import.py --check`: exit0,323 imported files verified;
  no source importer change.
- Rustfmt (`--edition 2024 --check`), shell syntax and diff checks: pass.
  These do not compile Rust or establish native semantic execution.

No Cargo while baseline65986 owns the build lane. No commit or push.
Independent read-only static review found no material findings in the bounded
authored slice; it confirmed complete comparison and retained source/API paths,
not native execution or fuel sufficiency. After the lane is released run
`cargo test -p suss-cli --test portable_chunk_effects --test portable_lazy_sequences --locked -- --test-threads=2`.
Retain exact failures and resolve actual unsupported dependencies before claiming
native matches. Full baseline and final-head CI remain later gates. Public
map/filter callback behavior, broader lazy operators and complete #17 remain open.
