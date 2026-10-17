# Fixed30M whole-source-child cost diagnostic

This is NOT default-fuel acceptance. Source/test behavior and all assertions are unchanged except one30M option update before each phase's existing outer request. Runtime-source and Macro-source each start at exactly30000000, finish the full body with16710494 remaining, and consume13289506 through the final-validation marker:3289506 above the original10M allowance **per separate phase**. Setup before request-start and untraced cleanup after the last marker are excluded. The original10M test remains failed and preserved; this diagnostic does not replace it.

Compile0; bootstrap0; exact full test1pass/0fail/0ignored;2915 inputs unchanged after four allowed generated outputs. Existing trace flags only. Sourcebody fully executes both physical lineage negatives and final forged-parent memo rejection in both source phases. Both regenerated Wasm ordered noncustom section sequences equal parent2bc35d1. No unclosed traced intervals; exact source/commands/receipts retained. Only memo-proof-cost-target used; no primary edits.

## Whole request: disjoint outer materializer-call intervals, EACH phase

| Existing call label | Count | Accepted fuel |
|---|---:|---:|
| hash-invocation-inclusive |84|4268879|
| domain-enrollment |930|3904303|
| constructor-and-native-fact |227|798340|
| ledger-publication |930|646946|
| scalar-and-native-fact |658|21135|
| array-and-native-fact |38|19136|
| All outer labelled calls | |9658739|
| Outside those call intervals | |3630767|
| Entire request through final validation | |13289506|

Hash includes guest arithmetic AND nested domain proofs; it is not guest-only cost. Outside calls includes logical proof, native projection/staging and other unlabelled work, not an exclusive measured native-allocation bucket. Do not add nested rows below to their parents.

## Existing inclusive native proof/capture intervals, EACH phase

Within the84 inclusive hash calls,168 domain passes cost storage/edges1518540, initial source-classes365400, canonical-method loops324504, prototype275856. Their sum2484300 is nested within hash4268879; the1784579 remainder is unlabelled staging/guest work, not classified purely arithmetic.

Within domain enrollment3904303: constructor snapshot captures227/1090144; constructor prototype227/606468; scalar edge/cut staging658/421120; scalar snapshot captures658/404502; scalar domain-node growth658/388549; array domain-node growth38/307205; constructor edge/cut staging227/261320; array snapshots38/248624; remaining smaller intervals are in whole-phase-native-census.json. These record phases are sequential within enrollment but their snapshot internal subphases overlap them.

## Actual traced requested native backing/copy work, EACH phase

These are accepted storage_snapshot::allocation<T> charges from raw before/after debit events, not size models or every native allocation. Separate requested new backing from prepaid relocation work (relocation is not another heap allocation). Allocator usable capacity is unknown.

| Traced type | Size | Requests | New backing bytes | Relocation-work bytes | Accepted byte debits |
|---|---:|---:|---:|---:|---:|
| Snapshot Node |136|1988|687616|146880|834496|
| Snapshot Atom |48|2530|530976|0|530976|
| Snapshot Cut |56|1046|181664|61152|242816|
| Promotion links Option<usize> |16|12|1536|0|1536|
| Total | |5576|1401792|208032|1609824|

Zero-count requests are retained in request counts. Node/Cut room requests occur in verified backing-then-relocation pairs; Atom buffers and promotion links have one backing request. Every accepted allocation debit exactly equals its requested count*size. Actual other native buffers (Form projection/decoding, domain entry vectors, roots, TryOrderedMap's internal allocations) lack complete typed events in this trace. Their byte totals cannot be fabricated or inferred by subtracting1.609824M from total request fuel. Snapshot allocation-debit intervals and type totals describe the SAME work, not additive categories. Largest explicitly traced backing type is Node, not Atom.

## Newly exposed tail, EACH phase

| Stage | Accepted fuel |
|---|---:|
| Physical child certificate checks |432562|
| Physical source inputs |182844|
| Normal recording bridge |12827|
| Private bridge |4|
| Physical materialization |715316|
| Wrong physical slot patch |2368|
| Factory finalization,5 calls including intended rejection |11434|
| Final parent, inclusive |2354033|
| Final validation |130642|

The original122240 remaining at physical-input entry cannot cover its full182844, and that first exhaustion hid several MILLION further units. Final-parent is the largest newly exposed stage. It partitions exactly into layout7604, child preparation1324174, recording12827, entry materialization278125, map construction545296, records extraction0, fresh expectation50965, finish135042. Child preparation contains324965 in labelled factories/enrollment and999209 outside those intervals. Map construction contains405730 inclusive hash work; physical materialize contains405202 inclusive hash work. These cannot be added again to final-parent/whole-phase totals.

## Actionable next attribution, not another blind small patch

Current evidence does not establish an exact dominant allocation/search INSIDE physical-inputs182844 or unlabelled child preparation999209: no internal scope markers. Source_root_inputs already batches child validation with shared done/audit; adding that batching is NOT a new opportunity. It still rebuilds independent native expectations and performs current source/storage checks. Certificate::validate_into checks certificate.source.witness.matches_logical_source; checked_into subsequently checks current expected.witness again. Equality of source-owner pointers is insufficient authority to skip either independent expected check.

Highest-value bounded next measurement: split existing checked_into current-witness call, validate_into logical/direct/recorded/class/prototype blocks, and native expectation projection/admission within these two large windows. Keep each original operation once and preserve errors/non-short-circuiting. Then consider whether repeated **actual decoding/allocation** can be shared in one effect-audited callback-free pass while still comparing every independent expected witness and retaining all source/storage facts. No cache/handoff or guard weakening is approved here. The smaller Vec<Option<Form>>→Vec<Form> duplicate in projection is real, but its scale is currently unknown and must not be presented as a3.3M solution. Current typed snapshot allocations also cannot alone justify a width-model claim of eliminating the full gap.

## Source-bound domain-node growth, EACH phase

The record node-growth intervals additionally identify85 actual positive growth events for write_domain::Node (distinct from storage_snapshot::Node136). Accepted debit768725 splits into85 existing growth controls,519040 requested new backing bytes and249600 prepaid relocation bytes. The unchanged charged source formula is `(new_capacity + old_len) * size_of::<Node>() + 1`; Node160 is source-bound from the scalar edge/cut charges and all growth events satisfy the capacity formula. This reconstruction is in domain-node-growth-census.json and domain_growth.py. It is accepted requested backing/copy work, not allocator physical traffic or an extra charge to add to enclosing enrollment. There are930 records but only85 positive growth events; empty/non-growing records are not claimed as allocations.
