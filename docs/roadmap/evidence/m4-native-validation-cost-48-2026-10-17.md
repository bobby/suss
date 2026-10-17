# Matched48 native-validation attribution

Diagnostic source commit `b460d1f81dfb717d6d21d02c49ee42d4c94120e9`, based on published `48cfd8378a295ce9fedd8306772c1a892aba518d`; four approved observer Rust files plus four regenerated bootstrap artifacts. No root-observation CSE. Approved observer patch SHA256 `907205b942747edf49a0ad54678e790a69624015f98e9cc45184150daade039c`. Only the original full fixture's fixed30M option differs from production allowance, after default Session/setup and once per separate Runtime-source/Macro-source outer request; no helper refill. Primary/V4 untouched.

Control and observer each passed compile, normal bootstrap and the one full unchanged Rust test (two source-phase bodies per test). All220 ordered old phase markers AND exact remaining-fuel balances match; zero differences. Every tuple has2915 frozen inputs, unchanged after its four allowed bootstrap outputs. Both tuples' ordered noncustom Wasm sections equal48. Source effects/counter placement were independently approved by Carver via main before execution. No source optimizations are introduced.

Control command wall: compile27.468s, bootstrap29.541s, test97.064s. Observer: compile22.496s, bootstrap21.883s, test76.039s. These are not benchmark improvements: observer overhead is unmetered, Cargo/cache/host load differ, and accepted fuel is exactly equal. Jobs2/Rayon2/testthreads2, sole memo-proof-cost-target. Exact commands/env flags/raw failures/partial scopes/bytes are retained. No extra suite or new experiment.

## Measured result, EACH separate source phase

Both control and observer start at30000000 and finish final validation at16723731: **13276269 fuel**, **3276269 above default10M**. Both Runtime and Macro results are identical. Setup before request-start and cleanup after final-validation marker are excluded. Historical d074/2bc cost13289506 differs by13237; that is a cross-tuple comparison with48field-helper/provenance changes, NOT observer savings. The old default-fuel failure remains distinct from this diagnostic pass.

| Disjoint whole-request partition | Fuel |
|---|---:|
| Union of outer labelled materializer calls |9645635|
| Outside those calls |3630634|
| Total through final-validation |13276269|

New outermost validation scopes account for2714910 of the3630634 outside-call units;915724 remains unlabelled there. Source classes are the dominant measured certification block:

| Native scope | Reached calls | Inclusive fuel |
|---|---:|---:|
| Certificate class/schema authority (`validate/classes`) |149|**2057064**|
| Certificate prototype origins |149|284353|
| Certificate recorded-input closure |149|153194|
| Certificate direct facts |149|80046|
| Certificate logical source comparison |149|35184|
| Independent current-witness comparison |99|18433|
| Constructor own schema snapshot |727|35452|
| Constructor memo-watch block |727|9254|
| Memo read-only method health |24|5472|
| Memo read-only constructor health |24|3024|
| Memo current-slot/receipt |24|288|

These inclusive scopes NEST (e.g. direct facts/constructor/memo/watch); do not sum all rows. `validate/classes` includes coverage/name/index/query and initial-class/schema authority work; this observer does not isolate its internal lookup versus snapshot costs. It is not proof that2057064 can be removed. Logical plus current comparisons total53617, confirming finite double-decoding is not the primary multi-million-unit opportunity on this tuple.

## The previously unexplained child-preparation window

Final-parent child preparation totals1324174, with324965 in labelled factories/enrollment and **999209 outside**. The new disjoint root validation blocks explain:

| Block outside factory/enrollment calls | Fuel |
|---|---:|
| Class/schema authority,23 calls |**713598**|
| Prototype origins,23 |68671|
| Recorded input closure,23 |17674|
| Direct facts,23 |10702|
| Logical source,23 |3337|
| Current witness,12 |2278|
| Sum explained |816260|
| Still unlabelled outside calls |182949|

Class/schema authority alone accounts for about71.4% of that999209 window. No effect-permission is inferred from the lack of guest callbacks: all class/source/storage/prototype/receipt checks remain required. Input projection or a root equality shortcut is not established as a fix.

Physical inputs cost182844 in the complete diagnostic: class checks58713; parent projection36186; token staging24286; child projection18192; prototype14826; recorded10497; layout7604; direct5676; logical2278/current2278. These disjoint outer blocks total180536, residual2308. This locates the default failing stage without claiming it is the whole remaining gap. Final-parent overall2352161; final-validation130642. No default10M success follows.

## Exact reached edge-attempt counts

Each phase has168 entered domain storage/edge passes. Counts are attempts reached by original control flow, including potential partial/error cases, not completed validity proofs. `ok=true` means Result::Ok, which includes false authority; no boolean-success count is inferred. No saturated counters in this execution.

| Original operation | Count |
|---|---:|
| Edge visits |18984|
| Immediate-value edges |7464|
| Exact node-root ref_eq attempts |605952|
| Node-root hits |11520|
| Child-root attempts/hits |0/0|
| Seed candidate/interior calls |0/0|

All11520 non-immediate edges hit recorded node roots, averaging52.6 exact comparisons/hit. Existing edge-membership fuel **624936 =18984 edge controls +605952 original node comparisons**, exact reconciliation. Native node snapshots cost876564; per-node controls17040 reconcile these disjoint parts to storage-and-edges1518540. These domain scopes are nested in hash calls, not additional whole-request costs. No extra ref query, root conversion, Weak upgrade, hash or charged layout field was added. No persistent gc_ref hash is proposed (GC relocation makes that unsafe). Counts bound the search opportunity; they do not establish net index savings after actual staging/lookup/cleanup fees.

## Assessment and stopping point

This is useful attribution progress, not production readiness. It replaces the unknown999209/3.63M diagnosis with a concrete dominant class-authority block and reconciled membership search costs, while ruling out spending more time on tiny finite-read/scoped-constructor subpaths. The unselected root-only experiment remains diagnostic-only. No next implementation or producer starts automatically. A future reviewed design must remove real work while preserving every authority/error/callback boundary; no cross-callback caching, source-fact stripping, guest-hash bypass or tariff/budget reduction is justified here.

The current complete diagnostic still exceeds default by3276269 per phase. No full workspace baseline, final-head CI, issue closure, PR, merge or M4 acceptance is claimed. Complete parsers report zero open scopes/negative residual/LIFO errors; raw interval rows and partial-result semantics are retained for independent recomputation. Observer-only unmetered overhead remains explicitly outside charged fuel equivalence.
