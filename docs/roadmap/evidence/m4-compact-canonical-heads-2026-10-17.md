# Compact canonical head storage: commands, results and limitations

Source `4a5919da8b1a0c5d98f99fdc84fbc0b911b3affe`, based on production `4c6545f78c23c15bba4219a142f69b01ed54e539`.
Three native Rust modules compact hash-table slots while preserving full 64-bit
hashes, canonical IDs, byte comparisons, collision chains and publication order.
Opaque nonzero indices come from checked canonical entry preparation. Encoding,
decoding, staging and remaining operations are prepaid. Neither rejected
flat-edge trial is incorporated; operation rates and allowances are unchanged.

The captured drivers run normal bootstrap generation, focused51, then
preservation76 under jobs1/Rayon1 with inherited RUSTFLAGS. Focused execution is
47 pass/four fuel failures. Preservation passes46 source/domain tests,17 core
imports, full map1, bitwise1 and analysis-graph11. Combined: **127 distinct tests,
123 pass/four fail, zero ignored or selected tests unexecuted**. The raw names,
commands, errors, source-derived selectors and independent reviews are retained.
All2,989 tested inputs match committed Git blobs and pinned ClojureScript.

Full pinned map retains69 ordered IDs:62 independent host passes/seven opaque
guest judgments, with no failed/skipped/unexecuted assertions. The pinned
70-observation bitwise test passes. Native tests cover hash0/MAX, encoding bounds,
collisions, staged growth and failure/cancellation recovery. Typed test error
injection is distinguished from actual Session interruption in the raw tests.

Measured native slots are24 versus16 bytes. The collision-heavy table fixture
charges762,356 versus748,317 units, including test-adapter encoding/decoding.
These14,039 removed units describe that fixture, not a full source request.
The exact old table is retained as an independent reference. Publication ledger
is12 per new canonical key on this source; earlier diagnostic ledger11 is historical.

The complete unchanged source-child fixture preserves all220 ordered labels and
results. Each Runtime/Macro request begins at10M and finishes successfully with
132,163 units, versus131,281 on4c: **882 actual units removed per phase**.

CoreEnv remains a failure before criterion observations. At the common `-pop`
checkpoint it retains384,029 versus155,099 units:228,930 prefix units removed.
It completes150 declarations versus136 and later fails at ASeq metadata.
This is partial-prefix evidence, not completed CoreEnv saving or a whole-cost
partition. Both record witnesses and the reduction witness also still exhaust
fuel before criterion observations. Their failed raw outcomes remain explicit.

Parent-only CI4c, run39066209035: foundation531 Rust passes/one ignored plus351
Python passes; integrations111 passes/14 same known fuel failures,225 of255
targets unexecuted. All10 logs/nine checkout proofs are retained. This is not a
candidate CI result or a full baseline. Current-source full workspace validation
and passing final-head CI remain required. No issue closure or milestone claim.

Rejected flat-edge experiments and the correction of their unused preservation
plan from72 to76 are [retained separately](https://github.com/bobby/suss/blob/860e2cefc167f58577257cc776906f340c9caeed/docs/roadmap/evidence/m4-flat-edge-plan-v3-v4-failed-trials-2026-10-17.md).
They add no current-source acceptance credit.

Evidence: [Git-resolved archive](m4-compact-canonical-heads-2026-10-17.tar.gz), SHA256 `9e8d0ed946c79f82347112c0d8eba4bd408443e322ae8a65f03cf649a4663fbd`;
134 members/133 manifested payloads.
The archive is **not standalone source**. GIT-SOURCE-RESOLUTION.json identifies
exact Suss and ClojureScript commit:path/blob/SHA256 for every frozen input,
including pre-bootstrap hybrid versions. The listed immutable Git objects are
required; repeated source/Wasm CAS stores and external evidence archives are not.
Source-only review snapshots and the retained legacy table are included explicitly.
