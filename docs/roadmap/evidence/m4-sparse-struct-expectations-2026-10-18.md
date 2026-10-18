# Sparse struct expectations: execution evidence

Source `4e21edfef1f61e39da563fbeeeb3abbf319e2793` stores only mutable struct slots in physical storage snapshots. Capture still reads/converts every field and recursively discovers every immutable reference. Matching still visits every captured node, including mutable descendants behind const-only intermediates. Exact retained object identity plus Wasmtime49.0.1 const-field enforcement makes const-slot rereads redundant. Captured field counts, mutable indexed values, arrays, slot hooks, root identity and independent source/class/prototype/memo authority remain checked. Initial I31/opaque-cut membership is retained separately; opaque descendants remain excluded.

This changes physical representation, not source or reuse authority. No guest callback, old-object repair, quota increase, tariff reduction or fixture/observation removal is introduced. Measured layouts: Node88, Atom48, mutable slot64, Snapshot112 bytes; do not substitute earlier estimates of a56-byte slot or larger node.

## Controlled measurements

The unchanged default-budget callable probe ran on clean published `25b4a57` with existing metadata/materialization trace flags. All3690tracked input hashes remained unchanged. All six requests completed51source roots then exhausted fuel constructing boundary26; no identity rows decoded. Its JSON matched the preceding untraced control exactly. Current inclusive buckets identify domain enrollment, snapshot capture and prototype work; nested buckets are not additive.

The candidate repeats the same probe, allowances, source observations and ordered51completed boundaries. Their completed construction prefixes spend219934–220603fewer fuel units, but all six requests still failboundary26. The raw errors now include a Wasm backtrace; they are not byte-identical to the control. Incomplete boundary totals, export/finalization and required identity observations remain unknown/unexecuted.

The complete source-child test passes twice on candidate tuples. All220ordered events/fivevariants/bothphases and final physical/memo negatives remain. Each phase finishes with1313487fuel rather than980173on25b4a57: **333314fewer units per complete request**. Not every nested substage improves; retain the raw adverse costs. These are fuel results, not wall-time claims. Initial candidate/control source-child drivers took62.77/44.07seconds; traced callable drivers126.82/86.14seconds. Timing conditions were not controlled, and durations do not establish a speed improvement.

## Commands and selected results

Shared target `/home/bobby/.cache/suss-m4-work/m4-cost-diagnostic-target`; jobs1/Rayon1 and inherited environment, without RUSTFLAGS overrides. Both normal candidate bootstrap regenerations and identity checks pass:

```sh
cargo run --profile test --locked -p suss-cli --bin suss-bootstrap -- runtime/bootstrap
python3 scripts/verify_bootstrap_identity.py
```

The initial tuple adds three snapshot tests; the final tuple adds a test-only membership-growth case and normally regenerated bootstrap identity outputs. All3690listed hashes stay unchanged during each run and the final supplemental tests. Initial snapshot cohort28PASS, growth1PASS, seed/method11PASS, fresh-proof1PASS, prototype8PASS in two selections, scoped/domain/callback17PASS, original Symbol/Keyword/environment5PASS, replaced-hash2PASS, constructor12PASS, complete source-childPASS, JankmapPASS, CLJS/decoder4PASS and core-import17PASS. The constructor group separately includes fresh-data classification; it is not another test in the fresh-proof filter.

Across the two tuples and supplements: **128 distinct selected test names:119PASS/9fuelFAIL/0ignored**. Raw executions total121PASS/10FAIL because source-child, the callable probe and a callback guard repeat. Results are not128executions of one identical compiled tuple. All stages and supplemental commands/results remain separately recorded.

New fixtures execute const-only ancestry to a mutable child, mutation/restoration, const host-write rejection, GC/exact-root replacement, I31/cut membership, unsupported late const references, root cleanup, sampled complete capture/match fuel failures and bounded internal IRQ charge sweeps. A six-member fixture crosses vector capacity4; sampled public capture fuel failures and70internal charge boundaries exercise growth/cleanup/retry. These are not exhaustive full API fuel, membership or retirement sweeps; direct internal capture seams do not cover every public entry precharge.

All48original memo rows of13fields match pinned expectations. Both original replaced-hash tests still pass their identity/value/effect assertions in both caller phases. Jankmap retains69ordered IDs:62host passes/seven guest-qualified judgments, no transport loss; opaque judgments are not promoted to independent host success. Four pinned CLJS property cases retain getter-effect/value observations afterGC in both phases; three decoder adversaries pass. No fresh upstream oracle or whole-suite compatibility is claimed.

## Failures and review limits

Both default-budget public identity tests and six coherent tests still fail fuel before required observations. Some original reader-route controls fail before reaching coherent/effect comparisons; passing unrelated guards do not substitute them.

The additional initializer replay also fails during Runtime caller-`&env` transport at1..39 before first returned-value/metadata/quote/effect assertions; its Macro iteration remains unexecuted. The identical test passed on production010e/fd78441, was unexecuted on239a4aa, then failed on25b4a57CI. It is a genuine prior-production regression newly visible relative to239, not a new fixture or a semantic pass. The sparse candidate has not repaired it; the coarse trap does not identify its responsible helper.

Independent design, exact-source and terminal reviews retain qualifications. Previously published25b4a57 CI39129238646 terminates FAILURE:548RustPASS/6fuelFAIL plus353PythonPASS foundation; integrations79PASS/19fuelFAIL. Both replaced-hash tests explicitlyPASS. Only24/259integration targets completed;235unexecuted. Nine actual checkout trees equal that head; tree checks do not prove per-job immutable inputs. Source4e21CI is separate and no passing final-head CI is established here.

Required full baseline remains unexecuted on this source while focused acceptance fails:

```sh
cargo test --workspace --locked -- --test-threads=2
```

Next: separately labelled higher-budget diagnostics may expose the currently unreachable tail and later semantic failures before selecting further cost changes. They cannot establish existing default-budget acceptance or change shipped allowances. Complete original issue criteria, full baseline, independent final-head review and passing final-head CI remain required. No issue closure, draft promotion or merge is established.

Raw packet: [archive](m4-sparse-struct-expectations-2026-10-18.tar.gz). Exact argv, environment overrides, source/bootstrap tuples, raw failures, control trace, observations and reviews are retained there.

Archive SHA256: `c88d6c36a6b10ee3c79ae00b35dbc3b0b3a346cfec7c2d3e5bb3fe1b5473e8ab` (135 members,134hashed payloads). Selected exact source/bootstrap images are included; common frozen inputs resolve through committed Git blobs. [Final committed-source/terminal review](m4-sparse-struct-expectations-final-review-2026-10-18.json) is retained separately from the archive.
