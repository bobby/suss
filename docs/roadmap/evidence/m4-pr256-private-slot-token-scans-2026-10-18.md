# Private-slot token scan: immutable negative filter and bounded small-token dispatch

Source commit: 12b88bcecf5e300360745c67ded9820f8afd9fc3. Tested production base is 6f0b2ece950a1158acd2f6059aab849f5c27b960. The final source and four normally regenerated bootstrap payloads come from V4 after-bootstrap snapshots and are checked against its executed and terminal owned hashes. No live-root source is substituted.

The earlier completed metadata-component diagnostic used a separate three-source observer overlay and an explicit 10B Macro allowance; caller defaults and semantic assertions remained unchanged. It completed all four expansions, one Rust diagnostic PASS, with 21,452 complete spans. About 95% of each second transport's observed cost was in controlled-parent execution. This includes frames, construction, cache/private placement and unlabelled work; it did not establish slot-token scans alone as dominant. Nested labelled costs are not additive with inclusive parent costs. The Macro caller's first expansion differed by 150 units from the uninstrumented c59 control (observed cost lower); the other three canonical chains matched exactly. Cause is unknown. No claim all four balances were identical or that observer timings are optimization evidence.

The proposed raw empty-array filter was rejected before implementation: private tokens are source-array OBJECT wrappers with mutable logical backing, not immutable-length raw arrays. That rejected idea supplies no bug-fix or test credit. V1 and V3 were prepared but UNEXECUTED. V2 actually passed 12 focused Rust tests across its two targets (not 12 newly added tests):

[{"stage": "private-slot", "pass": 7, "fail": 0, "ignored": 0, "seconds": 58.94273842702387, "log_sha256": "8c73cea057074141ab28bb044b1ca997ffdf922dd16a493371c4dd516e37aabf"}, {"stage": "reuse-guards", "pass": 5, "fail": 0, "ignored": 0, "seconds": 32.00253780401545, "log_sha256": "eb5c11f4fcc92e4125895b9cfc02be0b1e18a2fc7d4800354fe6cfb089ee53af"}]

V2's wide 64-token/100-input fixture cost 13,007 old versus 1,155 candidate units, but compatible-wrapper small cases regressed: T1 408 to 892, T0 193 to 673. These outcomes belong to V2, not V4, and motivated a corrected route.

V4 registers checked Session-owned four-field GC structs. It excludes only immutable kind/arity differences, never mutable source-array emptiness, descriptors or classes. Every compatible OBJECT still receives exact ref_eq comparisons in original order. No identity-hash index or authority/cache verdict is introduced. Only inputs' repeated token-owner scan is removed, using checked append-only insertion and immutable SessionValue identity; capture/apply owner checks, duplicate ID/reference guards, physical slot validation and incomplete fallback remain unchanged.

After checking ALL input owners, one prepaid three-unit dispatch chooses empty output for T0, the shared unfiltered const-generic loop for T1/T2, and immutable-shape filtering for larger token sets. Actual rates remain unchanged; removed operations no longer incur their old charges. Registration, dispatch and all added classification work are charged. The small compatible route may still pay a fixed per-call overhead; there is no universal saving claim.

Actual V4 stages, derived from its terminal receipt and successful nonzero Rust summaries:

[{"stage": "private-slot", "pass": 7, "fail": 0, "ignored": 0, "seconds": 50.12405722500989, "log_sha256": "7f4be8ab0c1bf1cf5b60785ea9ce7c38768ffc2ce08f5e61464b8e15b572f0ec"}, {"stage": "reuse-guards", "pass": 5, "fail": 0, "ignored": 0, "seconds": 30.30551907903282, "log_sha256": "89f4cceb84fffe08c953e65d616c25bbbc7346728b0ea5beddac1a3760f45068"}]

Actual V4 measured matrix (including adverse cases; unedited raw lines):

```text
private-slot-immutable-shape case=wide tokens=64 inputs=100 legacy=13007 candidate=1158
private-slot-immutable-shape case=compatible tokens=0 width=8 inputs=8 selected=0 legacy=17 candidate=12 delta=-5
private-slot-immutable-shape case=compatible tokens=0 width=96 inputs=96 selected=0 legacy=193 candidate=100 delta=-93
private-slot-immutable-shape case=compatible tokens=1 width=8 inputs=9 selected=1 legacy=56 candidate=58 delta=2
private-slot-immutable-shape case=compatible tokens=1 width=96 inputs=97 selected=1 legacy=408 candidate=410 delta=2
private-slot-immutable-shape case=compatible tokens=2 width=8 inputs=10 selected=2 legacy=129 candidate=130 delta=1
private-slot-immutable-shape case=compatible tokens=2 width=96 inputs=98 selected=2 legacy=657 candidate=658 delta=1
```

The source fixture proves genuine token backing mutation via aset followed by aget readback and exact identity with the inserted leaf, before GC and selection. Other focused checks cover raw/nonstruct and compatible empty/nonempty negatives, repeated token identities, duplicates, foreign log/value/late input, reset, bounded every-unit input-operation fuel cuts, deterministic IRQ at entry, cleanup/retry, and wide exact old/new selected ordinal/ID equality. Existing reuse/private-slot tests retain their original assertions. This is not exhaustive registration or mid-operation IRQ coverage, nor write rollback or general source certification.

Archive evidence keeps source variants and their normal bootstrap bytes separate. Repeated identical bytes are content-addressed once; MANIFEST maps every logical path to a safe regular payload and SHA256. The original diagnostic controller, raw logs, review corrections and setup history, parser, source snapshots, owner proof and independent reviews remain available. Observer success and V2 success are separate evidence and are not added to V4's overlapping focused count.

No unchanged default-budget CoreEnv success is established. Full-fixture V4 gain is UNKNOWN until a new complete diagnostic on the published source. No full baseline, final-head CI, issue completion, promotion or merge is claimed. Normal command arguments and environment are in each run.py/receipt; bootstrap command was cargo run --locked --profile test -p suss-cli --bin suss-bootstrap -- runtime/bootstrap, followed by target-aware scripts/verify_bootstrap_identity.py and the exact receipt test selectors. No RUSTFLAGS were set by these controllers.

[Raw evidence archive](m4-pr256-private-slot-token-scans-2026-10-18.tar.gz) — SHA256 `0646005bd790b1dc9dcf8803c62e45ba00b2464b7f86a6691f7f36362831dbd8`.

The archive contains 133 safe regular members, 132 content-addressed payloads and 266 logical file bindings. `MANIFEST.json` maps each logical path to its blob and digest.
