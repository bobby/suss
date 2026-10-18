# Fresh seed method data: execution evidence

Source `9d2f194` separates ordinary fresh seed data from pristine seed/reuse authority. The initial provisioning records exact descriptor, method-table root, protocol key and selected IHash value index. A replacement closure in that exact slot may be ordinary fresh data, but forces the result to remain uncached. Every other original snapshot node and slot remains checked. Strict seed matching, initial-reference membership and write-domain authority are unchanged. The replacement closure's environment is opaque data here; its shell type is not deep closure-integrity evidence.

The diagnostic on published base `239a4aa` first reproduced the original rejection: actual CanonicalSeed facts 20 and 51 failed fresh/non-memo checks. Restoring the original method restored strict matching in the separate seed probe. Its seed-only prototype query was vacuous and is not prototype acceptance evidence. Temporary tracing was removed before implementing the fix.

## Commands and results

Shared target `/home/bobby/.cache/suss-m4-work/m4-cost-diagnostic-target`, jobs1/Rayon1, inherited environment; no RUSTFLAGS override. Normal bootstrap command:

```sh
cargo run --profile test --locked -p suss-cli --bin suss-bootstrap -- runtime/bootstrap
python3 scripts/verify_bootstrap_identity.py
```

Both normal bootstrap/identity tuples pass. Full selection runs against a frozen precommit tree; the final tuple adds only one test and normally regenerated bootstrap identity outputs. All 3,687 listed hashes remain unchanged during each respective run. Exact argv, environment overrides, durations, manifests and logs are in the raw packet.

| Selection | Pass | Fail |
| --- | ---: | ---: |
| Seed/method selection before extra negative test | 10 | 0 |
| Scoped memo and retained child-method/callback negatives | 17 | 0 |
| Original Symbol memo variants | 2 | 0 |
| Original Keyword variants and actual environment effects | 3 | 0 |
| Both original replaced-hash integrations | 2 | 0 |
| Constructor inputs | 12 | 0 |
| Complete source-child witness | 1 | 0 |
| Pinned Jank map declaration | 1 | 0 |
| Pinned CLJS property observations/decoder negatives | 4 | 0 |
| Default-budget public namespace/callable identity | 0 | 2 |
| Coherent source/retirement cohort | 12 | 6 |
| Final seed/method selection including extra negative | 11 | 0 |

The callback-poisoning case appears in two selections, and ten seed/method tests repeat in the final tuple: **72 distinct tests, 64 pass, 8 fuel failures, 0 ignored**. Distinguish the two source/bootstrap tuples; the full cohort was not rerun after adding the test-only negative.

Both unchanged replaced-hash integrations previously failed non-memo certification and now pass all their original identity/value/memo/effect assertions in both caller phases. All 48 original Symbol/Keyword rows of 13 fields match their pinned expectations. The complete source-child request retains all 220 ordered events, five variants, both phases and final physical/memo negatives. The map test preserves its full 69 ordered assertion IDs at the existing allowance; host judgments and opaque guest judgments remain separate. Four pinned CLJS property cases retain value/getter-effect observations after GC in both phases; decoder adversaries remain required. No fresh upstream oracle execution or whole-suite compatibility is claimed.

New tests execute exact slot/key/table/value binding, replacement/GC, nonclosure and foreign rejection, all 429 direct-helper fuel cuts per phase with retry, entry IRQ recovery, sampled complete-seed cuts, absent authority, reset, independent clone rejection, payload/meta/count/UID/backing poison, and prototype-parent/unrelated-protocol poison with restoration. The full seed walk is not exhaustively fuel-swept. Duplicate/reordered tables and arbitrary old/new closure graph mutations are not all newly executed by this selection; unchanged snapshot coverage is not substituted for new execution evidence.

The two public identity tests and six coherent failures still exhaust unchanged fuel before their intended observations. Reader-route controls can fail before reaching coherent/effect comparisons. These failures do not prove namespace identity or callback acceptance. No tariff, quota, fixture, pinned expectation or source observation was reduced.

## Review and remaining gates

Independent implementation and additional-negative reviews found no blocking source defect; exact reviewed hashes and qualifications are retained. Published `239a4aa` CI `39125469326` terminated FAILURE: foundation 541 pass/6 fuel failures, Python 353 pass; integrations 71 pass/20 fail, including the two replaced-hash certification errors. Only 22/259 integration targets completed; 237 remain unexecuted. That CI is a historical control, not a result for this fix.

The required full baseline remains unexecuted on this source while focused acceptance fails:

```sh
cargo test --workspace --locked -- --test-threads=2
```

Next: measure current complete default-budget public identity requests before selecting further cost changes; older constructor-prefix measurements do not establish current residual cost. Preserve the complete #264 mutation/identity criteria and the remaining #241/#252/#257/#261/#270 requirements, then full baseline, independent final-head review and passing final-head CI. No issue closure, draft promotion or merge is established.

Raw packet: [archive](m4-fresh-seed-method-data-2026-10-18.tar.gz).

Archive SHA256: `583872929de21b6883e618c8b4144a0c4a446e36b06a4565ee9c1e7397b32d14` (85 members, 84 hashed payloads). Selected source/bootstrap images are included; remaining frozen inputs resolve through exact Git commit/blob bindings.

[Final committed-source review](m4-fresh-seed-method-data-final-review-2026-10-18.json) binds all seven source/bootstrap hashes at `9d2f194`; it is separate from the raw archive.
